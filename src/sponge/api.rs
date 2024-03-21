use ff::PrimeField;

/// This module implements a variant of the 'Secure Sponge API for Field Elements':  https://hackmd.io/bHgsH6mMStCVibM_wYvb2w
///
/// The API is defined by the `SpongeAPI` trait, which is implemented in terms of the `InnerSpongeAPI` trait.
/// `Neptune` provides implementations of `InnerSpongeAPI` for both `sponge::Sponge` and `sponge_circuit::SpongeCircuit`.
use crate::poseidon::{Arity, PoseidonConstants};

#[derive(Debug)]
pub enum Error {
    ParameterUsageMismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpongeOp {
    Absorb(u32),
    Squeeze(u32),
}

#[derive(Clone, Debug)]
pub struct IOPattern(pub Vec<SpongeOp>);

impl IOPattern {
    pub fn value(&self, domain_separator: u32) -> u128 {
        let mut hasher = Hasher::new();

        for op in self.0.iter() {
            hasher.update_op(*op);
        }
        hasher.finalize(domain_separator)
    }

    pub fn op_at(&self, i: usize) -> Option<&SpongeOp> {
        self.0.get(i)
    }
}

// A large 128-bit prime, per https://primes.utm.edu/lists/2small/100bit.html.
const HASHER_BASE: u128 = (0 - 159) as u128;

#[derive(Clone, Copy, Debug)]
pub(crate) struct Hasher {
    x: u128,
    x_i: u128,
    state: u128,
    current_op: SpongeOp,
}

impl Default for Hasher {
    fn default() -> Self {
        Self {
            x: HASHER_BASE,
            x_i: 1,
            state: 0,
            current_op: SpongeOp::Absorb(0),
        }
    }
}

impl Hasher {
    pub(crate) fn new() -> Self {
        Default::default()
    }

    /// Update hasher's current op to coalesce absorb/squeeze runs.
    pub(crate) fn update_op(&mut self, op: SpongeOp) {
        if self.current_op.matches(op) {
            self.current_op = self.current_op.combine(op)
        } else {
            self.finish_op();
            self.current_op = op;
        }
    }

    fn finish_op(&mut self) {
        if self.current_op.count() == 0 {
            return;
        };
        let op_value = self.current_op.value();

        self.update(op_value);
    }

    pub(crate) fn update(&mut self, a: u32) {
        self.x_i = self.x_i.overflowing_mul(self.x).0;
        self.state = self
            .state
            .overflowing_add(self.x_i.overflowing_mul(u128::from(a)).0)
            .0;
    }

    pub(crate) fn finalize(&mut self, domain_separator: u32) -> u128 {
        self.finish_op();
        self.update(domain_separator);
        self.state
    }
}

impl SpongeOp {
    pub const fn reset(&self) -> Self {
        match self {
            Self::Absorb(_) => Self::Squeeze(0),
            Self::Squeeze(_) => Self::Absorb(0),
        }
    }

    pub const fn count(&self) -> u32 {
        match self {
            Self::Absorb(n) | Self::Squeeze(n) => *n,
        }
    }

    pub const fn is_absorb(&self) -> bool {
        matches!(self, Self::Absorb(_))
    }

    pub const fn is_squeeze(&self) -> bool {
        matches!(self, Self::Squeeze(_))
    }

    pub fn combine(&self, other: Self) -> Self {
        assert!(self.matches(other));

        match self {
            Self::Absorb(n) => Self::Absorb(n + other.count()),
            Self::Squeeze(n) => Self::Squeeze(n + other.count()),
        }
    }

    pub const fn matches(&self, other: Self) -> bool {
        self.is_absorb() == other.is_absorb()
    }

    pub fn value(&self) -> u32 {
        match self {
            Self::Absorb(n) => {
                assert_eq!(0, n >> 31);
                n + (1 << 31)
            }
            Self::Squeeze(n) => {
                assert_eq!(0, n >> 31);
                *n
            }
        }
    }
}

pub trait SpongeAPI<F: PrimeField, A: Arity<F>> {
    type Acc;
    type Value;

    /// Optional `domain_separator` defaults to 0
    fn start(&mut self, p: IOPattern, domain_separator: Option<u32>, _: &mut Self::Acc);
    fn absorb(&mut self, length: u32, elements: &[Self::Value], acc: &mut Self::Acc);
    fn squeeze(&mut self, length: u32, acc: &mut Self::Acc) -> Vec<Self::Value>;
    fn finish(&mut self, _: &mut Self::Acc) -> Result<(), Error>;
}

pub trait InnerSpongeAPI<F: PrimeField, A: Arity<F>> {
    type Acc;
    type Value;

    fn initialize_capacity(&mut self, tag: u128, acc: &mut Self::Acc);
    fn read_rate_element(&mut self, offset: usize) -> Self::Value;
    fn add_rate_element(&mut self, offset: usize, x: &Self::Value);
    fn permute(&mut self, acc: &mut Self::Acc);

    // Supplemental methods needed for a generic implementation.
    fn rate(&self) -> usize;
    fn absorb_pos(&self) -> usize;
    fn squeeze_pos(&self) -> usize;
    fn set_absorb_pos(&mut self, pos: usize);
    fn set_squeeze_pos(&mut self, pos: usize);

    fn add(a: Self::Value, b: &Self::Value) -> Self::Value;

    fn initialize_state(&mut self, p_value: u128, acc: &mut Self::Acc) {
        self.initialize_capacity(p_value, acc);

        for i in 0..self.rate() {
            self.add_rate_element(i, &Self::zero());
        }
    }

    fn pattern(&self) -> &IOPattern;
    fn set_pattern(&mut self, pattern: IOPattern);

    fn increment_io_count(&mut self) -> usize;

    fn zero() -> Self::Value;
}

impl<F: PrimeField, A: Arity<F>, S: InnerSpongeAPI<F, A>> SpongeAPI<F, A> for S {
    type Acc = <S as InnerSpongeAPI<F, A>>::Acc;
    type Value = <S as InnerSpongeAPI<F, A>>::Value;

    fn start(&mut self, p: IOPattern, domain_separator: Option<u32>, acc: &mut Self::Acc) {
        let p_value = p.value(domain_separator.unwrap_or(0));

        self.set_pattern(p);
        self.initialize_state(p_value, acc);

        self.set_absorb_pos(0);
        self.set_squeeze_pos(0);
    }

    fn absorb(&mut self, length: u32, elements: &[Self::Value], acc: &mut Self::Acc) {
        assert_eq!(length as usize, elements.len());
        let rate = self.rate();

        for element in elements.iter() {
            if self.absorb_pos() == rate {
                self.permute(acc);
                self.set_absorb_pos(0);
            }
            let old = self.read_rate_element(self.absorb_pos());
            self.add_rate_element(self.absorb_pos(), &S::add(old, element));
            self.set_absorb_pos(self.absorb_pos() + 1);
        }
        let op = SpongeOp::Absorb(length);
        let old_count = self.increment_io_count();
        assert_eq!(Some(&op), self.pattern().op_at(old_count));

        self.set_squeeze_pos(rate);
    }

    fn squeeze(&mut self, length: u32, acc: &mut Self::Acc) -> Vec<Self::Value> {
        let rate = self.rate();

        let mut out = Vec::with_capacity(length as usize);

        for _ in 0..length {
            if self.squeeze_pos() == rate {
                self.permute(acc);
                self.set_squeeze_pos(0);
                self.set_absorb_pos(0);
            }
            out.push(self.read_rate_element(self.squeeze_pos()));
            self.set_squeeze_pos(self.squeeze_pos() + 1);
        }
        let op = SpongeOp::Squeeze(length);
        let old_count = self.increment_io_count();
        assert_eq!(Some(&op), self.pattern().op_at(old_count));

        out
    }

    fn finish(&mut self, acc: &mut Self::Acc) -> Result<(), Error> {
        // Clear state.
        self.initialize_state(0, acc);
        let final_io_count = self.increment_io_count();

        if final_io_count == self.pattern().0.len() {
            Ok(())
        } else {
            Err(Error::ParameterUsageMismatch)
        }
    }
}

#[cfg(test)]
mod test {
    use bellpepper::util_cs::test_shape_cs::TestShapeCS;
    use bellpepper_core::num::AllocatedNum;
    use bellpepper_core::test_cs::TestConstraintSystem;
    use bellpepper_core::ConstraintSystem;
    use blstrs::Scalar as Fr;
    use ff::{Field, PrimeFieldBits};
    use generic_array::typenum::U24;
    use serde::{Deserialize, Serialize};

    use crate::circuit2::Elt;
    use crate::sponge::circuit::SpongeCircuit;
    use crate::sponge::vanilla::Mode::Simplex;
    use crate::sponge::vanilla::SpongeTrait;

    use super::*;

    #[test]
    fn test_tag_values() {
        let test = |expected_value: u128, pattern: IOPattern, domain_separator: u32| {
            assert_eq!(expected_value, pattern.value(domain_separator));
        };

        test(0, IOPattern(vec![]), 0);
        test(
            340282366920938463463374607431768191899,
            IOPattern(vec![]),
            123,
        );
        test(
            340282366920938463463374607090318361668,
            IOPattern(vec![SpongeOp::Absorb(2), SpongeOp::Squeeze(2)]),
            0,
        );
        test(
            340282366920938463463374607090314341989,
            IOPattern(vec![SpongeOp::Absorb(2), SpongeOp::Squeeze(2)]),
            1,
        );
        test(
            340282366920938463463374607090318361668,
            IOPattern(vec![SpongeOp::Absorb(2), SpongeOp::Squeeze(2)]),
            0,
        );
        test(
            340282366920938463463374607090318361668,
            IOPattern(vec![
                SpongeOp::Absorb(1),
                SpongeOp::Absorb(1),
                SpongeOp::Squeeze(2),
            ]),
            0,
        );
        test(
            340282366920938463463374607090318361668,
            IOPattern(vec![
                SpongeOp::Absorb(1),
                SpongeOp::Absorb(1),
                SpongeOp::Squeeze(1),
                SpongeOp::Squeeze(1),
            ]),
            0,
        );
    }

    #[test]
    fn test_sponge_api_multiple_cs() {
        fn sponge_cycle<Scalar, CS: ConstraintSystem<Scalar>>(
            cs: &mut CS,
            elts: &[AllocatedNum<Scalar>],
        ) -> Vec<Elt<Scalar>>
        where
            Scalar: PrimeField + PrimeFieldBits + Serialize + for<'de> Deserialize<'de>,
        {
            let constant: PoseidonConstants<Scalar, U24> = PoseidonConstants::new();
            let mut ns = cs.namespace(|| "ns");
            let mut sponge = SpongeCircuit::new_with_constants(&constant, Simplex);
            let acc = &mut ns;
            let parameter = IOPattern(vec![
                SpongeOp::Absorb(elts.len() as u32),
                SpongeOp::Squeeze(1u32),
            ]);

            sponge.start(parameter, None, acc);
            SpongeAPI::absorb(
                &mut sponge,
                elts.len() as u32,
                &(0..elts.len())
                    .map(|i| Elt::Allocated(elts[i].clone()))
                    .collect::<Vec<Elt<Scalar>>>(),
                acc,
            );
            let output = SpongeAPI::squeeze(&mut sponge, 1, acc);
            sponge.finish(acc).unwrap();
            output
        }

        /*********************************
         * Test absorb w/ ShapeCS
         *********************************/
        let mut cs: TestShapeCS<Fr> = TestShapeCS::new();

        let elts = (0..10)
            .map(|i| {
                AllocatedNum::alloc(cs.namespace(|| format!("elt_{i}")), || Ok(Fr::ONE)).unwrap()
            })
            .collect::<Vec<_>>();

        // Values are `None`, we should get a None back
        let hash = sponge_cycle(&mut cs, &elts);
        assert!(hash[0].val().is_none());

        /*********************************
         * Test absorb w/ TestConstraintSystem
         *********************************/
        let mut cs: TestConstraintSystem<Fr> = TestConstraintSystem::new();

        let elts = (0..10)
            .map(|i| {
                AllocatedNum::alloc(cs.namespace(|| format!("elt_{i}")), || Ok(Fr::ONE)).unwrap()
            })
            .collect::<Vec<_>>();

        // We have values, we expect one in return
        let hash = sponge_cycle(&mut cs, &elts);
        assert_eq!(
            "Scalar(0x4d1f7863ee494536a938bd87d761a30828eeeeebfbc160117135dc6766f6e16c)",
            hash[0].val().unwrap().to_string()
        );
    }
}
