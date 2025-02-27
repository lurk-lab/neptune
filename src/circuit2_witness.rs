/// The `circuit2_witness` module implements witness-generation for the optimal Poseidon hash circuit.
use std::ops::{AddAssign, MulAssign};

use crate::circuit2::Elt;
use crate::hash_type::HashType;
use crate::matrix::Matrix;
use crate::mds::SparseMatrix;
use crate::poseidon::{Arity, Poseidon, PoseidonConstants};
use bellpepper::util_cs::witness_cs::SizedWitness;

use bellpepper_core::boolean::Boolean;
use bellpepper_core::num::{self, AllocatedNum};
use bellpepper_core::test_cs::TestConstraintSystem;
use bellpepper_core::{ConstraintSystem, LinearCombination, SynthesisError};
use ff::{Field, PrimeField};
use generic_array::sequence::GenericSequence;
use generic_array::typenum::Unsigned;
use generic_array::GenericArray;
use std::marker::PhantomData;

/// Create circuit for Poseidon hash, returning an `AllocatedNum` at the cost of one constraint.
pub fn poseidon_hash_allocated_witness<CS, Scalar, A>(
    cs: &mut CS,
    preimage: &[AllocatedNum<Scalar>],
    constants: &PoseidonConstants<Scalar, A>,
) -> Result<AllocatedNum<Scalar>, SynthesisError>
where
    CS: ConstraintSystem<Scalar>,
    Scalar: PrimeField,
    A: Arity<Scalar>,
{
    assert!(cs.is_witness_generator());
    let result = poseidon_hash_witness_into_cs(cs, preimage, constants);

    AllocatedNum::alloc(&mut cs.namespace(|| "result"), || Ok(result))
}

pub fn poseidon_hash_witness_into_cs<Scalar, A, CS>(
    cs: &mut CS,
    preimage: &[AllocatedNum<Scalar>],
    constants: &PoseidonConstants<Scalar, A>,
) -> Scalar
where
    CS: ConstraintSystem<Scalar>,
    Scalar: PrimeField,
    A: Arity<Scalar>,
{
    let scalar_preimage = preimage
        .iter()
        .map(|elt| elt.get_value().unwrap())
        .collect::<Vec<_>>();
    let mut p = Poseidon::new_with_preimage(&scalar_preimage, constants);

    p.generate_witness_into_cs(cs)
}

pub fn poseidon_hash_witness<Scalar, A>(
    preimage: &[AllocatedNum<Scalar>],
    constants: &PoseidonConstants<Scalar, A>,
) -> (Vec<Scalar>, Scalar)
where
    Scalar: PrimeField,
    A: Arity<Scalar>,
{
    let scalar_preimage = preimage
        .iter()
        .map(|elt| elt.get_value().unwrap())
        .collect::<Vec<_>>();

    poseidon_hash_scalar_witness(&scalar_preimage, constants)
}

pub fn poseidon_hash_scalar_witness<Scalar, A>(
    preimage: &[Scalar],
    constants: &PoseidonConstants<Scalar, A>,
) -> (Vec<Scalar>, Scalar)
where
    Scalar: PrimeField,
    A: Arity<Scalar>,
{
    let mut p = Poseidon::new_with_preimage(preimage, constants);

    let (aux, _inputs, result) = p.generate_witness();

    (aux, result)
}

impl<Scalar, A> SizedWitness<Scalar> for Poseidon<'_, Scalar, A>
where
    Scalar: PrimeField,
    A: Arity<Scalar>,
{
    fn num_constraints(&self) -> usize {
        let s_box_cost = 3;
        let width = A::ConstantsSize::to_usize();
        (width * s_box_cost * self.constants.full_rounds)
            + (s_box_cost * self.constants.partial_rounds)
    }

    fn num_inputs(&self) -> usize {
        0
    }

    fn num_aux(&self) -> usize {
        self.num_constraints()
    }
    fn generate_witness_into(&mut self, aux: &mut [Scalar], _inputs: &mut [Scalar]) -> Scalar {
        let width = A::ConstantsSize::to_usize();
        let constants = self.constants;
        let elements = &mut self.elements;

        let mut elements_buffer =
            GenericArray::<Scalar, A::ConstantsSize>::generate(|_| Scalar::ZERO);

        let c = &constants.compressed_round_constants;

        let mut offset = 0;
        let mut aux_index = 0;
        macro_rules! push_aux {
            ($val:expr) => {
                aux[aux_index] = $val;
                aux_index += 1;
            };
        }

        assert_eq!(width, elements.len());

        // First Round (Full)
        {
            // s-box
            for elt in elements.iter_mut() {
                let x = c[offset];
                let y = c[offset + width];
                let mut tmp = *elt;

                tmp.add_assign(x);
                tmp = tmp.square();
                push_aux!(tmp); // l2

                tmp = tmp.square();
                push_aux!(tmp); // l4

                tmp = tmp * (*elt + x) + y;
                push_aux!(tmp); // l5

                *elt = tmp;
                offset += 1;
            }
            offset += width; // post-round keys

            // mds
            {
                let m = &constants.mds_matrices.m;

                for j in 0..width {
                    let scalar_product = m
                        .iter()
                        .enumerate()
                        .fold(Scalar::ZERO, |acc, (n, row)| acc + (row[j] * elements[n]));

                    elements_buffer[j] = scalar_product;
                }
                elements.copy_from_slice(&elements_buffer);
            }
        }

        // Remaining initial full rounds.
        {
            for i in 1..constants.half_full_rounds {
                // Use pre-sparse matrix on last initial full round.
                let m = if i == constants.half_full_rounds - 1 {
                    &constants.pre_sparse_matrix
                } else {
                    &constants.mds_matrices.m
                };
                {
                    // s-box
                    for elt in elements.iter_mut() {
                        let y = c[offset];
                        let mut tmp = *elt;

                        tmp = tmp.square();
                        push_aux!(tmp); // l2

                        tmp = tmp.square();
                        push_aux!(tmp); // l4

                        tmp = tmp * *elt + y;
                        push_aux!(tmp); // l5

                        *elt = tmp;
                        offset += 1;
                    }
                }

                // mds
                {
                    for j in 0..width {
                        let scalar_product = m
                            .iter()
                            .enumerate()
                            .fold(Scalar::ZERO, |acc, (n, row)| acc + (row[j] * elements[n]));

                        elements_buffer[j] = scalar_product;
                    }
                    elements.copy_from_slice(&elements_buffer);
                }
            }
        }

        // Partial rounds
        {
            for i in 0..constants.partial_rounds {
                // s-box

                // FIXME: a little silly to use a loop here.
                for elt in elements[0..1].iter_mut() {
                    let y = c[offset];
                    let mut tmp = *elt;

                    tmp = tmp.square();
                    push_aux!(tmp); // l2

                    tmp = tmp.square();
                    push_aux!(tmp); // l4

                    tmp = tmp * *elt + y;
                    push_aux!(tmp); // l5

                    *elt = tmp;
                    offset += 1;
                }
                let m = &constants.sparse_matrixes[i];

                // sparse mds
                {
                    elements_buffer[0] = elements
                        .iter()
                        .zip(&m.w_hat)
                        .fold(Scalar::ZERO, |acc, (&x, &y)| acc + (x * y));

                    for j in 1..width {
                        elements_buffer[j] = elements[j] + elements[0] * m.v_rest[j - 1];
                    }

                    elements.copy_from_slice(&elements_buffer);
                }
            }
        }
        // Final full rounds.
        {
            let m = &constants.mds_matrices.m;
            for _ in 1..constants.half_full_rounds {
                {
                    // s-box
                    for elt in elements.iter_mut() {
                        let y = c[offset];
                        let mut tmp = *elt;

                        tmp = tmp.square();
                        push_aux!(tmp); // l2

                        tmp = tmp.square();
                        push_aux!(tmp); // l4

                        tmp = tmp * *elt + y;
                        push_aux!(tmp); // l5

                        *elt = tmp;
                        offset += 1;
                    }
                }

                // mds
                {
                    for j in 0..width {
                        let scalar_product = m
                            .iter()
                            .enumerate()
                            .fold(Scalar::ZERO, |acc, (n, row)| acc + (row[j] * elements[n]));

                        elements_buffer[j] = scalar_product;
                    }
                    elements.copy_from_slice(&elements_buffer);
                }
            }

            // Terminal full round
            {
                // s-box
                for elt in elements.iter_mut() {
                    let mut tmp = *elt;

                    tmp = tmp.square();
                    push_aux!(tmp); // l2

                    tmp = tmp.square();
                    push_aux!(tmp); // l4

                    tmp *= *elt;
                    push_aux!(tmp); // l5

                    *elt = tmp;
                }

                // mds
                {
                    for j in 0..width {
                        elements_buffer[j] =
                            (0..width).fold(Scalar::ZERO, |acc, i| acc + elements[i] * m[i][j]);
                    }
                    elements.copy_from_slice(&elements_buffer);
                }
            }
        }

        elements[1]
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::circuit2;
    use crate::poseidon::HashMode;
    use crate::{Poseidon, Strength};
    use bellpepper::util_cs::{witness_cs::WitnessCS, Comparable};
    use bellpepper_core::{test_cs::TestConstraintSystem, ConstraintSystem};
    use blstrs::Scalar as Fr;
    use generic_array::typenum;
    use rand::SeedableRng;
    use rand_xorshift::XorShiftRng;

    #[test]
    fn test_poseidon_hash_witness() {
        test_poseidon_hash_aux::<typenum::U2>(Strength::Standard, 237, false);
        test_poseidon_hash_aux::<typenum::U4>(Strength::Standard, 288, false);
        test_poseidon_hash_aux::<typenum::U8>(Strength::Standard, 387, false);
        test_poseidon_hash_aux::<typenum::U16>(Strength::Standard, 585, false);
        test_poseidon_hash_aux::<typenum::U24>(Strength::Standard, 777, false);
        test_poseidon_hash_aux::<typenum::U32>(Strength::Standard, 972, false);
        test_poseidon_hash_aux::<typenum::U36>(Strength::Standard, 1068, false);

        test_poseidon_hash_aux::<typenum::U2>(Strength::Strengthened, 279, false);
        test_poseidon_hash_aux::<typenum::U4>(Strength::Strengthened, 330, false);
        test_poseidon_hash_aux::<typenum::U8>(Strength::Strengthened, 432, false);
        test_poseidon_hash_aux::<typenum::U16>(Strength::Strengthened, 630, false);
        test_poseidon_hash_aux::<typenum::U24>(Strength::Strengthened, 822, false);
        test_poseidon_hash_aux::<typenum::U32>(Strength::Strengthened, 1017, false);
        test_poseidon_hash_aux::<typenum::U36>(Strength::Strengthened, 1113, false);

        test_poseidon_hash_aux::<typenum::U15>(Strength::Standard, 561, true);
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

    fn test_poseidon_hash_aux<A>(
        strength: Strength,
        expected_constraints: usize,
        constant_length: bool,
    ) where
        A: Arity<Fr>,
    {
        let mut rng = XorShiftRng::from_seed(crate::TEST_SEED);
        let arity = A::to_usize();
        let constants_x = if constant_length {
            PoseidonConstants::<Fr, A>::new_with_strength_and_type(
                strength,
                HashType::ConstantLength(arity),
            )
        } else {
            PoseidonConstants::<Fr, A>::new_with_strength(strength)
        };

        let range = if constant_length {
            1..=arity
        } else {
            arity..=arity
        };
        for preimage_length in range {
            let constants = if constant_length {
                constants_x.with_length(preimage_length)
            } else {
                constants_x.clone()
            };

            let expected_constraints_calculated = {
                let width = 1 + arity;
                let s_box_cost = 3;

                (width * s_box_cost * constants.full_rounds)
                    + (s_box_cost * constants.partial_rounds)
            };

            let mut data = |cs: &mut TestConstraintSystem<Fr>, fr_data: &mut [Fr]| {
                (0..preimage_length)
                    .map(|i| {
                        let fr = Fr::random(&mut rng);
                        fr_data[i] = fr;
                        AllocatedNum::alloc(cs.namespace(|| format!("data {}", i)), || Ok(fr))
                            .unwrap()
                    })
                    .collect::<Vec<_>>()
            };

            {
                let mut cs = TestConstraintSystem::<Fr>::new();
                let mut wcs = WitnessCS::<Fr>::new();

                let mut fr_data = vec![Fr::ZERO; preimage_length];
                let data: Vec<AllocatedNum<Fr>> = data(&mut cs, &mut fr_data);
                wcs.extend_aux(&fr_data);

                let _ = circuit2::poseidon_hash_allocated(&mut cs, data.clone(), &constants)
                    .expect("poseidon hashing failed");

                let out2 = poseidon_hash_allocated_witness(&mut wcs, &data, &constants)
                    .expect("poseidon hash witness generation failed");

                let cs_inputs = cs.scalar_inputs();
                let cs_aux = cs.scalar_aux();

                let wcs_inputs = wcs.input_assignment();
                let wcs_aux = wcs.aux_assignment();

                assert_eq!(cs_aux.len(), wcs_aux.len());

                assert_eq!(None, mismatch(&cs_inputs, wcs_inputs));
                dbg!(&cs_aux[220..], &wcs_aux[220..]);
                assert_eq!(None, mismatch(&cs_aux, wcs_aux));

                let mut p = Poseidon::<Fr, A>::new_with_preimage(&fr_data, &constants);
                let expected: Fr = p.hash_in_mode(HashMode::Correct);

                let expected_constraints_calculated = expected_constraints_calculated + 1;
                let expected_constraints = expected_constraints + 1;

                assert!(cs.is_satisfied(), "constraints not satisfied");

                assert_eq!(
                    expected,
                    out2.get_value().unwrap(),
                    "witness and non-circuit do not match"
                );

                assert_eq!(
                    expected_constraints_calculated,
                    cs.num_constraints(),
                    "constraint number miscalculated"
                );

                assert_eq!(
                    expected_constraints,
                    cs.num_constraints(),
                    "constraint number changed",
                );
            }
        }
    }
}
