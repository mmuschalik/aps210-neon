//! Calendar dates as day numbers (days since 1970-01-01), exact and dependency-free.

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        _ => 28,
    }
}

/// Add calendar months, clamping the day to the target month's length
/// (31 Aug + 6 months = 28/29 Feb), matching chrono's `checked_add_months`.
pub fn add_months(day: i64, months: u32) -> i64 {
    let (y, m, d) = civil_from_days(day);
    let total = (m - 1) + months;
    let (y2, m2) = (y + i64::from(total / 12), total % 12 + 1);
    days_from_civil(y2, m2, d.min(days_in_month(y2, m2)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_month_clamping() {
        for (y, m, d) in [(1970, 1, 1), (2026, 9, 30), (2000, 2, 29), (1999, 12, 31)] {
            assert_eq!(civil_from_days(days_from_civil(y, m, d)), (y, m, d));
        }
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        let aug31 = days_from_civil(2026, 8, 31);
        assert_eq!(civil_from_days(add_months(aug31, 6)), (2027, 2, 28));
        assert_eq!(civil_from_days(add_months(days_from_civil(2026, 9, 30), 12)), (2027, 9, 30));
    }
}
