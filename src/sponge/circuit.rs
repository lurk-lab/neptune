use crate::circuit2::{self, Elt, PoseidonCircuit2};
use crate::hash_type::HashType;
use crate::matrix::Matrix;
use crate::mds::SparseMatrix;
use crate::poseidon::{Arity, Poseidon, PoseidonConstants};
use crate::sponge::{
    api::{Hasher, IOPattern, InnerSpongeAPI, SpongeOp},
    vanilla::{Direction, Mode, SpongeTrait},
};
use crate::Strength;
use bellpepper::util_cs::witness_cs::{SizedWitness, WitnessCS};
use bellpepper_core::boolean::Boolean;
use bellpepper_core::num::{self, AllocatedNum};
use bellpepper_core::{ConstraintSystem, LinearCombination, Namespace, SynthesisError};
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
    pattern: IOPattern,
    io_count: usize,
    poseidon: Poseidon<'a, F, A>,
    shape_only: bool, // must be set to true if any value required for accurate witness-generation is missing.
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
            squeeze_pos: 0,
            permutation_count: 0,
            state: PoseidonCircuit2::new_empty::<CS>(constants),
            queue: VecDeque::with_capacity(A::to_usize()),
            pattern: IOPattern(Vec::new()),
            poseidon: Poseidon::new(constants),
            shape_only: false,
            io_count: 0,
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
        self.state.pos - 1
    }
    fn set_absorb_pos(&mut self, pos: usize) {
        self.state.pos = pos + 1;
    }

    fn element(&self, index: usize) -> Self::Elt {
        self.state.elements[index].clone()
    }

    fn set_element(&mut self, index: usize, elt: Self::Elt) {
        // If `elt` has no value, we are synthesizing. `self.poseidon.elements` is used only for witness-generation, so we
        // don't need to set in that case.
        if let Some(f) = elt.val() {
            self.poseidon.elements[index] = f;
        } else {
            // Since we failed to update the poseidon state, record that we must only be synthesizing shape.
            self.shape_only = true;
        }
        self.state.elements[index] = elt;
    }

    fn make_elt(&self, val: F, ns: &mut Self::Acc) -> Self::Elt {
        let allocated = AllocatedNum::alloc_infallible(ns, || val);
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
        self.state.apply_padding::<CS>();
    }

    fn permute_state(&mut self, ns: &mut Self::Acc) -> Result<(), Self::Error> {
        self.permutation_count += 1;

        if ns.is_witness_generator() {
            // Do not risk generating an inaccurate witness if a missing value has tainted our tracking of the concrete
            // state.
            assert!(!self.shape_only);
            self.poseidon.generate_witness_into_cs(ns);

            for (elt, scalar) in self
                .state
                .elements
                .iter_mut()
                .zip(self.poseidon.elements.iter())
            {
                *elt = Elt::num_from_fr::<CS>(*scalar);
            }
        } else {
            self.state
                .hash(&mut ns.namespace(|| format!("permutation {}", self.permutation_count)))?;
        };

        Ok(())
    }

    fn enqueue(&mut self, elt: Self::Elt) {
        self.queue.push_back(elt);
    }
    fn dequeue(&mut self) -> Option<Self::Elt> {
        self.queue.pop_front()
    }

    fn squeeze_aux(&mut self) -> Self::Elt {
        let squeezed = self.element(SpongeTrait::squeeze_pos(self) + SpongeTrait::capacity(self));
        SpongeTrait::set_squeeze_pos(self, SpongeTrait::squeeze_pos(self) + 1);

        squeezed
    }

    fn absorb_aux(&mut self, elt: &Self::Elt) -> Self::Elt {
        // Elt::add always returns `Ok`, so `unwrap` is safe.
        self.element(SpongeTrait::absorb_pos(self) + SpongeTrait::capacity(self))
            .add_ref(elt)
            .unwrap()
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

impl<'a, F: PrimeField, A: Arity<F>, CS: 'a + ConstraintSystem<F>> InnerSpongeAPI<F, A>
    for SpongeCircuit<'a, F, A, CS>
{
    type Acc = Namespace<'a, F, CS>;
    type Value = Elt<F>;

    fn initialize_capacity(&mut self, tag: u128, _acc: &mut Self::Acc) {
        let mut repr = F::Repr::default();
        repr.as_mut()[..16].copy_from_slice(&tag.to_le_bytes());

        let f = F::from_repr(repr).unwrap();
        let elt = Elt::num_from_fr::<Self::Acc>(f);
        self.set_element(0, elt);
    }

    fn read_rate_element(&mut self, offset: usize) -> Self::Value {
        self.element(offset + SpongeTrait::capacity(self))
    }
    fn add_rate_element(&mut self, offset: usize, x: &Self::Value) {
        self.set_element(offset + SpongeTrait::capacity(self), x.clone());
    }
    fn permute(&mut self, acc: &mut Self::Acc) {
        SpongeTrait::permute(self, acc).unwrap();
    }

    // Supplemental methods needed for a generic implementation.

    fn zero() -> Elt<F> {
        Elt::num_from_fr::<CS>(F::ZERO)
    }

    fn rate(&self) -> usize {
        SpongeTrait::rate(self)
    }
    fn absorb_pos(&self) -> usize {
        SpongeTrait::absorb_pos(self)
    }
    fn squeeze_pos(&self) -> usize {
        SpongeTrait::squeeze_pos(self)
    }
    fn set_absorb_pos(&mut self, pos: usize) {
        SpongeTrait::set_absorb_pos(self, pos);
    }
    fn set_squeeze_pos(&mut self, pos: usize) {
        SpongeTrait::set_squeeze_pos(self, pos);
    }
    fn add(a: Elt<F>, b: &Elt<F>) -> Elt<F> {
        a.add_ref(b).unwrap()
    }

    fn pattern(&self) -> &IOPattern {
        &self.pattern
    }

    fn set_pattern(&mut self, pattern: IOPattern) {
        self.pattern = pattern
    }

    fn increment_io_count(&mut self) -> usize {
        let old_count = self.io_count;
        self.io_count += 1;
        old_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sponge::vanilla::Sponge;

    use bellpepper::util_cs::witness_cs::WitnessCS;
    use bellpepper_core::{test_cs::TestConstraintSystem, Circuit};
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
            allocated_elements.push(Elt::Allocated(
                AllocatedNum::alloc(&mut ns.namespace(|| format!("elt{}", i)), || Ok(element))
                    .unwrap(),
            ));
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

        let permutation_constraints = 288; // For U4.
        let permutations_per_direction = (n - 1) / A::to_usize();
        let final_absorption_permutation = 1;
        let expected_permutations = 2 * permutations_per_direction + final_absorption_permutation;
        let expected_constraints = permutation_constraints * expected_permutations;

        assert_eq!(expected_permutations, circuit.permutation_count);
        assert_eq!(expected_constraints, root_cs.num_constraints());
        // Simple sanity check that results are all non-zero and distinct.
        for (i, elt) in allocated_result.iter().enumerate() {
            assert!(elt.val().unwrap() != F::ZERO);
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
            .absorb(
                &Elt::Allocated(AllocatedNum::alloc_infallible(&mut ns, || {
                    F::from(n as u64)
                })),
                &mut ns,
            )
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
                let elt = Elt::Allocated(AllocatedNum::alloc_infallible(
                    &mut ns.namespace(|| format!("{}", i)),
                    || f,
                ));
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

    #[test]
    fn test_sponge_api_circuit_simple() {
        use crate::sponge::api::SpongeAPI;

        let parameter = IOPattern(vec![
            SpongeOp::Absorb(1),
            SpongeOp::Absorb(5),
            SpongeOp::Squeeze(3),
        ]);

        let mut cs = TestConstraintSystem::<Fr>::new();
        let mut wcs = WitnessCS::<Fr>::new();
        let circuit_output = {
            let p = Sponge::<Fr, typenum::U5>::api_constants(Strength::Standard);

            let mut sponge = SpongeCircuit::new_with_constants(&p, Mode::Simplex);
            let mut wsponge = SpongeCircuit::new_with_constants(&p, Mode::Simplex);

            let mut ns = cs.namespace(|| "ns");
            let acc = &mut ns;

            let mut wns = wcs.namespace(|| "ns");
            let wacc = &mut wns;

            sponge.start(parameter.clone(), None, acc);
            wsponge.start(parameter.clone(), None, wacc);
            sponge.state.debug();
            wsponge.state.debug();
            SpongeAPI::absorb(
                &mut sponge,
                1,
                &[Elt::num_from_fr::<TestConstraintSystem<Fr>>(Fr::from(123))],
                acc,
            );
            SpongeAPI::absorb(
                &mut wsponge,
                1,
                &[Elt::num_from_fr::<WitnessCS<Fr>>(Fr::from(123))],
                wacc,
            );
            sponge.state.debug();
            wsponge.state.debug();

            SpongeAPI::absorb(
                &mut sponge,
                5,
                &[
                    Elt::num_from_fr::<TestConstraintSystem<Fr>>(Fr::from(123)),
                    Elt::num_from_fr::<TestConstraintSystem<Fr>>(Fr::from(123)),
                    Elt::num_from_fr::<TestConstraintSystem<Fr>>(Fr::from(123)),
                    Elt::num_from_fr::<TestConstraintSystem<Fr>>(Fr::from(123)),
                    Elt::num_from_fr::<TestConstraintSystem<Fr>>(Fr::from(123)),
                ],
                acc,
            );
            SpongeAPI::absorb(
                &mut wsponge,
                5,
                &[
                    Elt::num_from_fr::<WitnessCS<Fr>>(Fr::from(123)),
                    Elt::num_from_fr::<WitnessCS<Fr>>(Fr::from(123)),
                    Elt::num_from_fr::<WitnessCS<Fr>>(Fr::from(123)),
                    Elt::num_from_fr::<WitnessCS<Fr>>(Fr::from(123)),
                    Elt::num_from_fr::<WitnessCS<Fr>>(Fr::from(123)),
                ],
                wacc,
            );
            sponge.state.debug();
            wsponge.state.debug();

            dbg!("last squeeze");
            let output = SpongeAPI::squeeze(&mut sponge, 3, acc);
            let woutput = SpongeAPI::squeeze(&mut wsponge, 3, wacc);

            sponge.state.debug();
            wsponge.state.debug();

            sponge.finish(acc).unwrap();
            wsponge.finish(wacc).unwrap();

            let output_scalars = output.iter().map(|x| x.val()).collect::<Vec<_>>();
            let woutput_scalars = woutput.iter().map(|x| x.val()).collect::<Vec<_>>();
            assert_eq!(output_scalars, woutput_scalars);

            output
        };
        let cs_aux = cs.scalar_aux();
        let wcs_aux = wcs.aux_assignment();
        let a = &cs_aux[300..320];
        let b = &wcs_aux[300..320];
        dbg!(cs_aux.len(), wcs_aux.len(), a, b);

        assert_eq!(None, mismatch(&cs_aux, wcs_aux));
        assert!(cs.is_satisfied());

        let non_circuit_output = {
            let p = Sponge::<Fr, typenum::U5>::api_constants(Strength::Standard);
            let mut sponge = Sponge::new_with_constants(&p, Mode::Simplex);
            let acc = &mut ();

            sponge.start(parameter, None, acc);
            SpongeAPI::absorb(&mut sponge, 1, &[Fr::from(123)], acc);
            SpongeAPI::absorb(
                &mut sponge,
                5,
                &[
                    Fr::from(123),
                    Fr::from(123),
                    Fr::from(123),
                    Fr::from(123),
                    Fr::from(123),
                ],
                acc,
            );

            let output = SpongeAPI::squeeze(&mut sponge, 3, acc);

            sponge.finish(acc).unwrap();

            output
        };

        assert_eq!(non_circuit_output.len(), circuit_output.len());
        for (circuit, non_circuit) in circuit_output.iter().zip(non_circuit_output) {
            assert_eq!(circuit.val().unwrap(), non_circuit);
        }
    }

    #[test]
    #[should_panic]
    fn test_sponge_api_circuit_failure() {
        use crate::sponge::api::SpongeAPI;

        let parameter = IOPattern(vec![
            SpongeOp::Absorb(1),
            SpongeOp::Absorb(5),
            SpongeOp::Squeeze(3),
        ]);

        {
            let p = Sponge::<Fr, typenum::U5>::api_constants(Strength::Standard);
            let mut sponge = SpongeCircuit::new_with_constants(&p, Mode::Simplex);
            let mut cs = TestConstraintSystem::<Fr>::new();
            let mut ns = cs.namespace(|| "ns");
            let acc = &mut ns;

            sponge.start(parameter, None, acc);
            SpongeAPI::absorb(
                &mut sponge,
                1,
                &[Elt::num_from_fr::<TestConstraintSystem<Fr>>(Fr::from(123))],
                acc,
            );
            SpongeAPI::absorb(
                &mut sponge,
                4,
                &[
                    Elt::num_from_fr::<TestConstraintSystem<Fr>>(Fr::from(123)),
                    Elt::num_from_fr::<TestConstraintSystem<Fr>>(Fr::from(123)),
                    Elt::num_from_fr::<TestConstraintSystem<Fr>>(Fr::from(123)),
                    Elt::num_from_fr::<TestConstraintSystem<Fr>>(Fr::from(123)),
                ],
                acc,
            );

            let _ = SpongeAPI::squeeze(&mut sponge, 3, acc);
        }
    }

    #[test]
    fn test_sponge_api_circuit() {
        for i in 1..3 {
            for j in 1..3 {
                test_sponge_api_circuit_aux::<Fr, typenum::U2>(i, j);
            }
        }
    }

    fn test_sponge_api_circuit_aux<F: PrimeField, A: Arity<F>>(
        absorb_count: usize,
        squeeze_count: usize,
    ) {
        use crate::sponge::api::SpongeAPI;

        let arity = A::to_usize();
        let expected_absorb_permutations = 1 + (absorb_count - 1) / arity;
        let expected_squeeze_permutations = (squeeze_count - 1) / arity;
        let expected_permutations = expected_absorb_permutations + expected_squeeze_permutations;

        let parameter = IOPattern(vec![
            SpongeOp::Absorb(absorb_count as u32),
            SpongeOp::Squeeze(squeeze_count as u32),
        ]);

        let circuit_output = {
            let p = Sponge::<Fr, typenum::U5>::api_constants(Strength::Standard);
            let mut sponge = SpongeCircuit::new_with_constants(&p, Mode::Simplex);
            let mut cs = TestConstraintSystem::<Fr>::new();
            let mut ns = cs.namespace(|| "ns");
            let acc = &mut ns;

            let elts: Vec<_> =
                std::iter::repeat(Elt::num_from_fr::<TestConstraintSystem<Fr>>(Fr::from(123)))
                    .take(absorb_count)
                    .collect();

            sponge.start(parameter.clone(), None, acc);

            SpongeAPI::absorb(&mut sponge, absorb_count as u32, &elts[..], acc);

            let output = SpongeAPI::squeeze(&mut sponge, squeeze_count as u32, acc);

            sponge.finish(acc).unwrap();

            assert_eq!(expected_permutations, sponge.permutation_count);

            output
        };

        let non_circuit_output = {
            let p = Sponge::<Fr, typenum::U5>::api_constants(Strength::Standard);
            let mut sponge = Sponge::new_with_constants(&p, Mode::Simplex);
            let acc = &mut ();

            let elts: Vec<_> = std::iter::repeat(Fr::from(123))
                .take(absorb_count)
                .collect();

            sponge.start(parameter, None, acc);
            SpongeAPI::absorb(&mut sponge, absorb_count as u32, &elts[..], acc);

            let output = SpongeAPI::squeeze(&mut sponge, squeeze_count as u32, acc);

            sponge.finish(acc).unwrap();

            output
        };

        assert_eq!(non_circuit_output.len(), circuit_output.len());
        for (circuit, non_circuit) in circuit_output.iter().zip(non_circuit_output) {
            assert_eq!(circuit.val().unwrap(), non_circuit);
        }
    }

    struct S<F: PrimeField, A: Arity<F>> {
        p: PoseidonConstants<F, A>,
    }

    impl<F: PrimeField, A: Arity<F>> Circuit<F> for S<F, A> {
        fn synthesize<CS: ConstraintSystem<F>>(self, cs: &mut CS) -> Result<(), SynthesisError> {
            use crate::sponge::api::SpongeAPI;

            let mut sponge = SpongeCircuit::<F, A, _>::new_with_constants(&self.p, Mode::Simplex);
            let mut ns = cs.namespace(|| "ns");

            let a1 = AllocatedNum::alloc_infallible(&mut ns.namespace(|| "a1"), || F::from(1));
            let a2 = AllocatedNum::alloc_infallible(&mut ns.namespace(|| "a2"), || F::from(2));
            let a3 = AllocatedNum::alloc_infallible(&mut ns.namespace(|| "a3"), || F::from(3));
            let a4 = AllocatedNum::alloc_infallible(&mut ns.namespace(|| "a4"), || F::from(4));
            let a5 = AllocatedNum::alloc_infallible(&mut ns.namespace(|| "a5"), || F::from(4));

            let acc = &mut ns;

            let parameter = IOPattern(vec![SpongeOp::Absorb(5), SpongeOp::Squeeze(1)]);

            sponge.start(parameter, None, acc);

            SpongeAPI::absorb(
                &mut sponge,
                5,
                &[
                    Elt::Allocated(a1),
                    Elt::Allocated(a2),
                    Elt::Allocated(a3),
                    Elt::Allocated(a4),
                    Elt::Allocated(a5),
                ],
                acc,
            );

            let _squeezed = SpongeAPI::squeeze(&mut sponge, 1, acc);

            sponge.finish(acc).unwrap();

            Ok(())
        }
    }

    fn test_sponge_synthesis_aux<F: PrimeField, A: Arity<F>>() {
        let p: PoseidonConstants<F, A> = Sponge::api_constants(Strength::Standard);
        let s = S { p: p.clone() };
        let s2 = S { p };

        let mut cs = TestConstraintSystem::<F>::new();
        let mut wcs = WitnessCS::<F>::new();

        s.synthesize(&mut cs).unwrap();
        s2.synthesize(&mut wcs).unwrap();
    }

    #[test]
    fn test_sponge_synthesis() {
        test_sponge_synthesis_aux::<Fr, typenum::U2>();
        test_sponge_synthesis_aux::<Fr, typenum::U4>();
    }

    // Returns index of first mismatch, along with the mismatched elements if they exist.
    #[allow(clippy::type_complexity)]
    fn mismatch<T: PartialEq + Copy>(a: &[T], b: &[T]) -> Option<(usize, (Option<T>, Option<T>))> {
        for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
            if x != y {
                return Some((i, (Some(*x), Some(*y))));
            }
        }
        use std::cmp::Ordering;

        match a.len().cmp(&b.len()) {
            Ordering::Less => Some((a.len(), (None, Some(b[a.len()])))),
            Ordering::Greater => Some((b.len(), (Some(a[b.len()]), None))),
            Ordering::Equal => None,
        }
    }
}
