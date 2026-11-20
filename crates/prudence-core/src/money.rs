//! Money tagged with currency and valuation basis (design.txt §4.A.1).
//!
//! LCR and MLH use market value; NSFR uses carrying value net of specific
//! provisions. Mixing bases, or currencies without an explicit conversion, is a
//! compile-time error: `Money<Aud, MarketValue> + Money<Aud, CarryingValue>`
//! does not type-check.

use std::fmt;
use std::iter::Sum;
use std::marker::PhantomData;
use std::ops::{Add, AddAssign, Neg, Sub};

use num_traits::{Signed, Zero};

use crate::num::{Q, display};

pub trait Currency: Copy + fmt::Debug + Default {
    const CODE: &'static str;
}
pub trait Basis: Copy + fmt::Debug + Default {
    const NAME: &'static str;
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Aud;
impl Currency for Aud {
    const CODE: &'static str = "AUD";
}

/// Carrying value (AASB), net of specific provisions: the NSFR basis and the
/// balance of a deposit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CarryingValue;
impl Basis for CarryingValue {
    const NAME: &'static str = "carrying value";
}

/// Market value: the LCR/MLH basis for liquid assets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MarketValue;
impl Basis for MarketValue {
    const NAME: &'static str = "market value";
}

pub struct Money<C: Currency, B: Basis> {
    amount: Q,
    _tag: PhantomData<(C, B)>,
}

impl<C: Currency, B: Basis> Money<C, B> {
    pub fn new(amount: Q) -> Self {
        Money { amount, _tag: PhantomData }
    }
    pub fn zero() -> Self {
        Self::new(Q::zero())
    }
    pub fn amount(&self) -> &Q {
        &self.amount
    }
    pub fn is_zero(&self) -> bool {
        self.amount.is_zero()
    }
    pub fn is_negative(&self) -> bool {
        self.amount.is_negative()
    }
    pub fn min(self, other: Self) -> Self {
        if self.amount <= other.amount { self } else { other }
    }
    /// Apply a rate (run-off, haircut, factor). The result keeps the basis.
    pub fn times(&self, rate: &Q) -> Self {
        Self::new(&self.amount * rate)
    }
}

impl<C: Currency, B: Basis> Clone for Money<C, B> {
    fn clone(&self) -> Self {
        Self::new(self.amount.clone())
    }
}
impl<C: Currency, B: Basis> PartialEq for Money<C, B> {
    fn eq(&self, o: &Self) -> bool {
        self.amount == o.amount
    }
}
impl<C: Currency, B: Basis> Eq for Money<C, B> {}
impl<C: Currency, B: Basis> PartialOrd for Money<C, B> {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}
impl<C: Currency, B: Basis> Ord for Money<C, B> {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.amount.cmp(&o.amount)
    }
}
impl<C: Currency, B: Basis> fmt::Debug for Money<C, B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} ({})", C::CODE, display(&self.amount), B::NAME)
    }
}
impl<C: Currency, B: Basis> fmt::Display for Money<C, B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", C::CODE, display(&self.amount))
    }
}
impl<C: Currency, B: Basis> Add for Money<C, B> {
    type Output = Self;
    fn add(self, o: Self) -> Self {
        Self::new(self.amount + o.amount)
    }
}
impl<'a, C: Currency, B: Basis> Add<&'a Money<C, B>> for &'a Money<C, B> {
    type Output = Money<C, B>;
    fn add(self, o: Self) -> Money<C, B> {
        Money::new(&self.amount + &o.amount)
    }
}
impl<C: Currency, B: Basis> AddAssign<&Money<C, B>> for Money<C, B> {
    fn add_assign(&mut self, o: &Self) {
        self.amount += &o.amount;
    }
}
impl<C: Currency, B: Basis> Sub for Money<C, B> {
    type Output = Self;
    fn sub(self, o: Self) -> Self {
        Self::new(self.amount - o.amount)
    }
}
impl<'a, C: Currency, B: Basis> Sub<&'a Money<C, B>> for &'a Money<C, B> {
    type Output = Money<C, B>;
    fn sub(self, o: Self) -> Money<C, B> {
        Money::new(&self.amount - &o.amount)
    }
}
impl<C: Currency, B: Basis> Neg for Money<C, B> {
    type Output = Self;
    fn neg(self) -> Self {
        Self::new(-self.amount)
    }
}
impl<C: Currency, B: Basis> Sum for Money<C, B> {
    fn sum<I: Iterator<Item = Self>>(it: I) -> Self {
        it.fold(Self::zero(), |a, b| a + b)
    }
}
impl<'a, C: Currency, B: Basis> Sum<&'a Money<C, B>> for Money<C, B> {
    fn sum<I: Iterator<Item = &'a Self>>(it: I) -> Self {
        it.fold(Self::zero(), |a, b| a + b.clone())
    }
}

/// Conservation check (design.txt §4.A.2): `parts` must add back to `whole`
/// exactly and be non-negative. Returns a description of the violation.
pub fn conserve<C: Currency, B: Basis>(whole: &Money<C, B>, parts: &[Money<C, B>]) -> Result<(), String> {
    if let Some(p) = parts.iter().find(|p| p.is_negative()) {
        return Err(format!("negative part {p}"));
    }
    let total: Money<C, B> = parts.iter().sum();
    if &total != whole {
        return Err(format!("parts sum to {total}, expected {whole}"));
    }
    Ok(())
}

pub type AudCarrying = Money<Aud, CarryingValue>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::num::int;

    #[test]
    fn arithmetic_and_conservation() {
        let a = AudCarrying::new(int(100));
        let parts = [AudCarrying::new(int(60)), AudCarrying::new(int(40))];
        assert!(conserve(&a, &parts).is_ok());
        assert!(conserve(&a, &[AudCarrying::new(int(60))]).is_err());
        assert!(conserve(&a, &[AudCarrying::new(int(110)), AudCarrying::new(int(-10))]).is_err());
        assert_eq!(a.times(&Q::new(1.into(), 4.into())), AudCarrying::new(int(25)));
    }
}
