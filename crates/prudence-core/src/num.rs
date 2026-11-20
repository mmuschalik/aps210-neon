//! Exact numbers. Core semantics use rationals (design.txt §4.A.1): no IEEE-754
//! floats anywhere, and no rounding until a value leaves the system.

use num_bigint::BigInt;
use num_integer::Integer;
use num_rational::BigRational;
use num_traits::{One, Signed, Zero};

/// An exact rational number.
pub type Q = BigRational;

pub fn int(n: i64) -> Q {
    Q::from_integer(BigInt::from(n))
}

/// Parse a decimal literal such as `-12.345` or `0.85` exactly.
pub fn parse_decimal(s: &str) -> Option<Q> {
    let s = s.trim();
    let (neg, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    if body.is_empty() {
        return None;
    }
    let (whole, frac) = body.split_once('.').unwrap_or((body, ""));
    if whole.is_empty() && frac.is_empty() {
        return None;
    }
    if !whole.chars().all(|c| c.is_ascii_digit()) || !frac.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let digits = format!("{whole}{frac}");
    let numer: BigInt = if digits.is_empty() { BigInt::zero() } else { digits.parse().ok()? };
    let denom = BigInt::from(10u32).pow(frac.len() as u32);
    let q = Q::new(numer, denom);
    Some(if neg { -q } else { q })
}

/// Round to `places` decimal places, half-to-even, and format. Used only at the
/// boundary (display, submission); never inside a calculation.
pub fn to_decimal_string(q: &Q, places: u32) -> String {
    let scale = BigInt::from(10u32).pow(places);
    let scaled = q * Q::from_integer(scale.clone());
    let rounded = round_half_even(&scaled);
    let neg = rounded.is_negative();
    let abs = rounded.abs();
    let (int_part, frac_part) = abs.div_rem(&scale);
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    out.push_str(&int_part.to_string());
    if places > 0 {
        let frac = frac_part.to_string();
        out.push('.');
        out.push_str(&"0".repeat(places as usize - frac.len()));
        out.push_str(&frac);
    }
    out
}

/// Exact decimal representation if one exists (denominator of the form 2^a 5^b),
/// otherwise rounded to `fallback_places` with a trailing `…`.
pub fn display(q: &Q) -> String {
    let mut d = q.denom().clone();
    let mut places = 0u32;
    let two = BigInt::from(2u32);
    let five = BigInt::from(5u32);
    let (mut twos, mut fives) = (0u32, 0u32);
    while (&d % &two).is_zero() {
        d /= &two;
        twos += 1;
    }
    while (&d % &five).is_zero() {
        d /= &five;
        fives += 1;
    }
    if d.is_one() {
        places = places.max(twos).max(fives);
        let s = to_decimal_string(q, places);
        return s;
    }
    format!("{}…", to_decimal_string(q, 12))
}

fn round_half_even(x: &Q) -> BigInt {
    let floor = x.floor().to_integer();
    let frac = x - Q::from_integer(floor.clone());
    let half = Q::new(BigInt::one(), BigInt::from(2u32));
    if frac > half || (frac == half && floor.is_odd()) {
        floor + 1
    } else {
        floor
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_decimals_exactly() {
        assert_eq!(parse_decimal("0.85").unwrap(), Q::new(85.into(), 100.into()));
        assert_eq!(parse_decimal("-12.5").unwrap(), Q::new((-25).into(), 2.into()));
        assert_eq!(parse_decimal("7").unwrap(), int(7));
        assert!(parse_decimal("1e5").is_none());
        assert!(parse_decimal("").is_none());
    }

    #[test]
    fn rounds_half_even_at_the_boundary() {
        assert_eq!(to_decimal_string(&parse_decimal("2.25").unwrap(), 1), "2.2");
        assert_eq!(to_decimal_string(&parse_decimal("2.35").unwrap(), 1), "2.4");
        assert_eq!(to_decimal_string(&parse_decimal("-0.05").unwrap(), 1), "0.0");
        assert_eq!(to_decimal_string(&parse_decimal("-0.15").unwrap(), 1), "-0.2");
        assert_eq!(display(&parse_decimal("46.75").unwrap()), "46.75");
        assert_eq!(display(&Q::new(2.into(), 3.into())), "0.666666666667…");
    }
}
