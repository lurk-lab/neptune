#![cfg_attr(not(feature = "std"), no_std)]
#![allow(dead_code)]
#![allow(unused_imports)]

extern crate alloc;
pub use alloc::vec::Vec;

pub use crate::poseidon::{Arity, Poseidon};
use crate::round_constants::generate_constants;
use crate::round_numbers::{round_numbers_base, round_numbers_strengthened};
#[cfg(feature = "abomonation")]
use abomonation_derive::Abomonation;
#[cfg(test)]
use blstrs::Scalar as Fr;
pub use error::Error;
use ff::PrimeField;
use generic_array::GenericArray;
#[cfg(feature = "std")]
use serde::{Deserialize, Serialize};
use trait_set::trait_set;

// See https://www.lurklurk.org/effective-rust/re-export.html
pub use generic_array;

#[cfg(all(
    any(feature = "cuda", feature = "opencl"),
    not(any(
        feature = "arity2",
        feature = "arity4",
        feature = "arity8",
        feature = "arity11",
        feature = "arity16",
        feature = "arity24",
        feature = "arity36"
    ))
))]
compile_error!("The `cuda` and `opencl` features need at least one arity feature to be set");

#[cfg(all(
    feature = "strengthened",
    not(any(feature = "cuda", feature = "opencl"))
))]
compile_error!("The `strengthened` feature needs the `cuda` and/or `opencl` feature to be set");

#[cfg(all(
    any(feature = "cuda", feature = "opencl"),
    not(any(feature = "bls", feature = "pasta",))
))]
compile_error!("The `cuda` and `opencl` features need the `bls` and/or `pasta` feature to be set");

/// Poseidon circuit
#[cfg(feature = "std")]
pub mod circuit;
#[cfg(feature = "std")]
pub mod circuit2;
#[cfg(feature = "std")]
pub mod circuit2_witness;
pub mod error;
mod matrix;
mod mds;

/// Poseidon hash
pub mod poseidon;
mod poseidon_alt;
mod preprocessing;
mod round_constants;
mod round_numbers;

/// Sponge
#[cfg(any(test, feature = "std"))]
pub mod sponge;

/// Hash types and domain separation tags.
pub mod hash_type;

/// Tree Builder
#[cfg(any(feature = "cuda", feature = "opencl"))]
pub mod tree_builder;

/// Column Tree Builder
#[cfg(any(feature = "cuda", feature = "opencl"))]
pub mod column_tree_builder;

/// Batch Hasher
#[cfg(any(feature = "cuda", feature = "opencl"))]
pub mod batch_hasher;

#[cfg(any(feature = "cuda", feature = "opencl"))]
pub mod proteus;

#[cfg(not(any(feature = "cuda", feature = "opencl")))]
trait_set! {
   /// Use a trait alias, so that we can have different traits depending on the features.
   ///
   /// When `cuda` and/or `opencl` is enabled, then the field also needs to implement `GpuName`.
   //pub trait NeptuneField = PrimeField + ec_gpu::GpuName;
   pub trait NeptuneField = PrimeField;
}

#[cfg(any(feature = "cuda", feature = "opencl"))]
trait_set! {
   /// Use a trait alias, so that we can have different traits depending on the features.
   ///
   /// When `cuda` and/or `opencl` is enabled, then the field also needs to implement `GpuName`.
   pub trait NeptuneField = PrimeField + ec_gpu::GpuName;
}

#[cfg(feature = "std")]
mod serde_impl;

pub(crate) const TEST_SEED: [u8; 16] = [
    0x59, 0x62, 0xbe, 0x5d, 0x76, 0x3d, 0x31, 0x8d, 0x17, 0xdb, 0x37, 0x32, 0x54, 0x06, 0xbc, 0xe5,
];

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "abomonation", derive(Abomonation))]
pub enum Strength {
    Standard,
    Strengthened,
}

impl core::fmt::Display for Strength {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Standard => write!(f, "standard"),
            Self::Strengthened => write!(f, "strengthened"),
        }
    }
}

pub(crate) const DEFAULT_STRENGTH: Strength = Strength::Standard;

pub trait BatchHasher<F, A>
where
    F: PrimeField,
    A: Arity<F>,
{
    // type State;

    fn hash(&mut self, preimages: &[GenericArray<F, A>]) -> Result<Vec<F>, Error>;

    fn hash_into_slice(
        &mut self,
        target_slice: &mut [F],
        preimages: &[GenericArray<F, A>],
    ) -> Result<(), Error> {
        assert_eq!(target_slice.len(), preimages.len());
        // FIXME: Account for max batch size.

        target_slice.copy_from_slice(self.hash(preimages)?.as_slice());
        Ok(())
    }

    /// `max_batch_size` is advisory. Implenters of `BatchHasher` should ensure that up to the returned max hashes can
    /// be safely performed on the target GPU (currently 2080Ti). The max returned should represent a safe batch size
    /// optimized for performance.
    /// `BatchHasher` users are responsible for not attempting to hash batches larger than the advised maximum.
    fn max_batch_size(&self) -> usize {
        700000
    }
}

pub fn round_numbers(arity: usize, strength: &Strength) -> (usize, usize) {
    match strength {
        Strength::Standard => round_numbers_base(arity),
        Strength::Strengthened => round_numbers_strengthened(arity),
    }
}

#[cfg(test)]
pub(crate) fn scalar_from_u64s(parts: [u64; 4]) -> Fr {
    let mut le_bytes = [0u8; 32];
    le_bytes[0..8].copy_from_slice(&parts[0].to_le_bytes());
    le_bytes[8..16].copy_from_slice(&parts[1].to_le_bytes());
    le_bytes[16..24].copy_from_slice(&parts[2].to_le_bytes());
    le_bytes[24..32].copy_from_slice(&parts[3].to_le_bytes());
    let mut repr = <Fr as PrimeField>::Repr::default();
    repr.as_mut().copy_from_slice(&le_bytes[..]);
    Fr::from_repr_vartime(repr).expect("u64s exceed BLS12-381 scalar field modulus")
}

const SBOX: u8 = 1; // x^5
const FIELD: u8 = 1; // Gf(p)

fn round_constants<F: PrimeField>(arity: usize, strength: &Strength) -> Vec<F> {
    let t = arity + 1;

    let (full_rounds, partial_rounds) = round_numbers(arity, strength);

    let r_f = full_rounds as u16;
    let r_p = partial_rounds as u16;

    let fr_num_bits = F::NUM_BITS;
    let field_size = {
        assert!(u16::try_from(fr_num_bits).is_ok());
        // It's safe to convert to u16 for compatibility with other types.
        fr_num_bits as u16
    };

    generate_constants::<F>(FIELD, SBOX, field_size, t as u16, r_f, r_p)
}

/// Apply the quintic S-Box (s^5) to a given item
pub(crate) fn quintic_s_box<F: PrimeField>(l: &mut F, pre_add: Option<&F>, post_add: Option<&F>) {
    if let Some(x) = pre_add {
        l.add_assign(x);
    }
    let mut tmp = *l;
    tmp = tmp.square(); // l^2
    tmp = tmp.square(); // l^4
    l.mul_assign(&tmp); // l^5
    if let Some(x) = post_add {
        l.add_assign(x);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strengthened_round_numbers() {
        let cases = [
            (1, 69),
            (2, 69),
            (3, 70),
            (4, 70),
            (5, 70),
            (6, 70),
            (7, 72),
            (8, 72),
            (9, 72),
            (10, 72),
            (11, 72),
            (16, 74),
            (24, 74),
            (36, 75),
            (64, 77),
        ];

        cases.iter().for_each(|(arity, expected_rounds)| {
            let (full_rounds, actual_rounds) = round_numbers_strengthened(*arity);
            assert_eq!(8, full_rounds);
            assert_eq!(
                *expected_rounds, actual_rounds,
                "wrong number of partial rounds for arity {}",
                *arity
            );
        })
    }
}
