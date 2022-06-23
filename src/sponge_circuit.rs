use crate::circuit2::{self, Elt, PoseidonCircuit2};
use crate::hash_type::HashType;
use crate::matrix::Matrix;
use crate::mds::SparseMatrix;
use crate::poseidon::{Arity, Poseidon, PoseidonConstants};
use crate::sponge::{Direction, Mode, SpongeTrait};
use bellperson::gadgets::boolean::Boolean;
use bellperson::gadgets::num::{self, AllocatedNum};
use bellperson::{ConstraintSystem, LinearCombination, Namespace, SynthesisError};
use ff::{Field, PrimeField};
use std::collections::VecDeque;
use std::marker::PhantomData;

pub struct SpongeCircuit<'a, F, A, C>
where
    F: PrimeField,
    A: Arity<F>,
    C: ConstraintSystem<F>,
{
    constants: &'a PoseidonConstants<F, A>,
    mode: Mode,
    direction: Direction,
    absorbed: usize,
    squeezed: usize,
    squeeze_pos: usize,
    permutation_count: usize,
    state: PoseidonCircuit2<'a, F, A>,
    queue: VecDeque<Elt<F>>,
    permutation_circuits: Vec<PoseidonCircuit2<'a, F, A>>,
    _c: PhantomData<C>,
}

impl<'a, F: PrimeField, A: Arity<F>, CS: 'a + ConstraintSystem<F>> SpongeTrait<'a, F, A>
    for SpongeCircuit<'a, F, A, CS>
{
    type Acc = Namespace<'a, F, CS>;
    type Elt = Elt<F>;
    type Error = SynthesisError;

    fn new_with_constants(constants: &'a PoseidonConstants<F, A>, mode: Mode) -> Self {
        Self {
            mode,
            direction: Direction::Absorbing,
            constants,
            absorbed: 0,
            squeezed: 0,
            squeeze_pos: 1,
            permutation_count: 0,
            state: PoseidonCircuit2::new_empty::<CS>(constants),
            queue: VecDeque::with_capacity(A::to_usize()),
            permutation_circuits: Default::default(),
            _c: Default::default(),
        }
    }

    fn mode(&self) -> Mode {
        self.mode
    }
    fn direction(&self) -> Direction {
        self.direction
    }
    fn set_direction(&mut self, direction: Direction) {
        self.direction = direction;
    }
    fn absorbed(&self) -> usize {
        self.absorbed
    }
    fn set_absorbed(&mut self, absorbed: usize) {
        self.absorbed = absorbed;
    }
    fn squeezed(&self) -> usize {
        self.squeezed
    }
    fn set_squeezed(&mut self, squeezed: usize) {
        self.squeezed = squeezed;
    }
    fn squeeze_pos(&self) -> usize {
        self.squeeze_pos
    }
    fn set_squeeze_pos(&mut self, squeeze_pos: usize) {
        self.squeeze_pos = squeeze_pos;
    }
    fn absorb_pos(&self) -> usize {
        self.state.pos
    }
    fn set_absorb_pos(&mut self, pos: usize) {
        self.state.pos = pos;
    }

    fn element(&self, index: usize) -> Self::Elt {
        self.state.elements[index].clone()
    }
    fn set_element(&mut self, index: usize, elt: Self::Elt) {
        self.state.elements[index] = elt;
    }

    fn make_elt(&self, val: F, ns: &mut Self::Acc) -> Self::Elt {
        let allocated = AllocatedNum::alloc(ns, || Ok(val)).unwrap();
        Elt::Allocated(allocated)
    }

    fn rate(&self) -> usize {
        A::to_usize()
    }

    fn capacity(&self) -> usize {
        1
    }

    fn size(&self) -> usize {
        self.constants.width()
    }

    fn constants(&self) -> &PoseidonConstants<F, A> {
        self.constants
    }

    fn pad(&mut self) {
        self.state.apply_padding();
    }

    fn permute_state(&mut self, ns: &mut Self::Acc) -> Result<(), Self::Error> {
        self.permutation_count += 1;
        self.state
            .hash(&mut ns.namespace(|| format!("permutation {}", self.permutation_count)))?;
        Ok(())
    }

    fn enqueue(&mut self, elt: Self::Elt) {
        self.queue.push_back(elt);
    }
    fn dequeue(&mut self) -> Option<Self::Elt> {
        self.queue.pop_front()
    }

    fn squeeze_aux(&mut self) -> Self::Elt {
        let squeezed = self.element(self.squeeze_pos());
        self.set_squeeze_pos(self.squeeze_pos() + 1);

        squeezed
    }

    fn absorb_aux(&mut self, elt: &Self::Elt) -> Self::Elt {
        // Elt::add always returns `Ok`, so `unwrap` is safe.
        self.element(self.absorb_pos()).add(elt.clone()).unwrap()
    }

    fn squeeze_elements(&mut self, count: usize, ns: &mut Self::Acc) -> Vec<Self::Elt> {
        let mut elements = Vec::with_capacity(count);
        for _ in 0..count {
            if let Ok(Some(squeezed)) = self.squeeze(ns) {
                elements.push(squeezed);
            }
        }
        elements
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sponge::Sponge;
    use bellperson::util_cs::test_cs::TestConstraintSystem;
    use blstrs::Scalar as Fr;
    use generic_array::typenum;
    use rand::{Rng, SeedableRng};
    use rand_xorshift::XorShiftRng;
    use std::collections::HashSet;

    #[test]
    fn test_simplex_circuit() {
        let mut rng = XorShiftRng::from_seed(crate::TEST_SEED);

        // Exercise duplex sponges with eventual size less, equal to, and greater to rate.
        for size in 2..10 {
            test_simplex_aux::<Fr, typenum::U4, _>(&mut rng, size);
        }
    }

    fn test_simplex_aux<F: PrimeField, A: Arity<F>, R: Rng>(rng: &mut R, n: usize) {
        let c = Sponge::<F, A>::simplex_constants(n);

        let mut circuit = SpongeCircuit::new_with_constants(&c, Mode::Simplex);
        let mut cs = TestConstraintSystem::<F>::new();
        let mut ns = cs.namespace(|| "ns");

        let mut sponge = Sponge::new_with_constants(&c, Mode::Simplex);
        let acc = &mut ();

        let mut elements = Vec::with_capacity(n);
        let mut allocated_elements = Vec::with_capacity(n);

        for i in 0..n {
            let element = F::random(&mut *rng);
            elements.push(element);
            allocated_elements
                .push(circuit.make_elt(element, &mut ns.namespace(|| format!("elt{}", i))));
        }

        sponge.absorb_elements(elements.as_slice(), acc).unwrap();
        circuit
            .absorb_elements(allocated_elements.as_slice(), &mut ns)
            .unwrap();

        let result = sponge.squeeze_elements(n, acc);
        let allocated_result = circuit.squeeze_elements(n, &mut ns);

        let root_cs = ns.get_root();

        assert!(root_cs.is_satisfied());
        assert_eq!(result.len(), allocated_result.len());

        result
            .iter()
            .zip(&allocated_result)
            .all(|(a, b)| *a == b.val().unwrap());

        let permutation_constraints = 285; // For U4.
        let permutations_per_direction = (n - 1) / A::to_usize();
        let final_absorption_permutation = 1;
        let expected_permutations = 2 * permutations_per_direction + final_absorption_permutation;
        let expected_constraints = permutation_constraints * expected_permutations;

        assert_eq!(expected_permutations, circuit.permutation_count);
        assert_eq!(expected_constraints, root_cs.num_constraints());
        // Simple sanity check that results are all non-zero and distinct.
        for (i, elt) in allocated_result.iter().enumerate() {
            assert!(elt.val().unwrap() != F::zero());
            // This is expensive (n^2), but it's hard to put field element into a set since we can't hash or compare (except equality).
            for (j, elt2) in allocated_result.iter().enumerate() {
                if i != j {
                    assert!(elt.val() != elt2.val());
                }
            }
        }

        assert_eq!(n, elements.len());
        assert_eq!(n, allocated_result.len());
    }

    #[test]
    fn test_sponge_duplex_circuit_consistency() {
        let mut rng = XorShiftRng::from_seed(crate::TEST_SEED);

        // Exercise duplex sponges with eventual size less, equal to, and greater to rate.
        for size in 4..10 {
            test_duplex_consistency_aux::<Fr, typenum::U8, _>(&mut rng, size, 10);
        }

        // Exercise duplex sponges with eventual size less, equal to, and greater than multiples of rate.
        for _ in 0..10 {
            let size = rng.gen_range(15..25);
            test_duplex_consistency_aux::<Fr, typenum::U4, _>(&mut rng, size, 10);
        }

        // Use very small rate to ensure exercising edge cases.
        for _ in 0..10 {
            let size = rng.gen_range(15..25);
            test_duplex_consistency_aux::<Fr, typenum::U2, _>(&mut rng, size, 10);
        }
    }

    fn test_duplex_consistency_aux<F: PrimeField, A: Arity<F>, R: Rng>(
        rng: &mut R,
        n: usize,
        trials: usize,
    ) {
        let mut output = None;
        let mut signatures = HashSet::new();

        for _ in 0..trials {
            let (o, sig) = test_duplex_consistency_inner::<F, A, R>(rng, n);
            signatures.insert(sig);
            if let Some(output) = output {
                assert_eq!(output, o);
            };
            output = Some(o);
        }
        // Make sure many different paths were taken.
        assert!(trials as f64 > 0.9 * signatures.len() as f64);
    }

    fn test_duplex_consistency_inner<F: PrimeField, A: Arity<F>, R: Rng>(
        rng: &mut R,
        n: usize,
    ) -> (Vec<F>, Vec<bool>) {
        let c = Sponge::<F, A>::duplex_constants();

        let mut circuit = SpongeCircuit::new_with_constants(&c, Mode::Duplex);
        let mut cs = TestConstraintSystem::<F>::new();
        let mut ns = cs.namespace(|| "ns");

        let mut sponge = Sponge::new_with_constants(&c, Mode::Duplex);
        let acc = &mut ();

        // Reminder: a duplex sponge should encode its length as a prefix.
        sponge.absorb(&F::from(n as u64), acc).unwrap();
        circuit
            .absorb(&circuit.make_elt(F::from(n as u64), &mut ns), &mut ns)
            .unwrap();

        let mut output = Vec::with_capacity(n);
        let mut circuit_output = Vec::with_capacity(n);

        let mut signature = Vec::with_capacity(n);
        let mut i = 0;

        while output.len() < n {
            let try_to_squeeze: bool = rng.gen();
            signature.push(try_to_squeeze);

            if try_to_squeeze {
                if let Ok(Some(squeezed)) = sponge.squeeze(acc) {
                    output.push(squeezed);

                    let x = circuit.squeeze(&mut ns).unwrap();
                    circuit_output.push(x);
                }
            } else {
                let f = F::from(sponge.absorbed() as u64);
                sponge.absorb(&f, acc).unwrap();
                i += 1;
                let elt = circuit.make_elt(f, &mut ns.namespace(|| format!("{}", i)));
                circuit.absorb(&elt, &mut ns).unwrap();
            }
        }

        assert_eq!(n, output.len());
        assert_eq!(output.len(), circuit_output.len());

        for (a, b) in output.iter().zip(circuit_output) {
            assert_eq!(*a, b.unwrap().val().unwrap());
        }

        (output, signature)
    }
}