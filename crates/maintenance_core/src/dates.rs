//! Calendar dates as `#[deprecated_after]` writes them: `YYYY-MM-DD`, in UTC.

/// Days since 1970-01-01 of `YYYY-MM-DD`, or why it does not read.
pub fn parse_days(text: &str) -> Result<i64, String> {
    let parts: Vec<&str> = text.split('-').collect();
    let [y, m, d] = parts[..] else {
        return Err(format!("`{text}` is not YYYY-MM-DD"));
    };
    if y.len() != 4 || m.len() != 2 || d.len() != 2 || ![y, m, d].iter().all(|p| p.bytes().all(|b| b.is_ascii_digit())) {
        return Err(format!("`{text}` is not YYYY-MM-DD"));
    }
    let (y, m, d): (i64, u32, u32) = (y.parse().unwrap_or(0), m.parse().unwrap_or(0), d.parse().unwrap_or(0));
    if !(1..=12).contains(&m) || d == 0 || d > days_in_month(y, m) {
        return Err(format!("`{text}` is not a day of the calendar"));
    }
    Ok(days_from_civil(y, m, d))
}

/// Today, in days since 1970-01-01 (UTC), by this machine's clock.
pub fn today_days() -> i64 {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    (secs / 86_400) as i64
}

/// Days since 1970-01-01 as `YYYY-MM-DD`.
pub fn show_days(days: i64) -> String {
    // Howard Hinnant's civil_from_days
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        2 if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_read_and_show_both_ways() {
        assert_eq!(parse_days("1970-01-01"), Ok(0));
        assert_eq!(parse_days("2026-10-19"), Ok(20_745));
        assert_eq!(parse_days("2024-02-29"), Ok(19_782));
        for days in [-1, 0, 59, 365, 11_016, 20_745, 47_482] {
            assert_eq!(parse_days(&show_days(days)), Ok(days));
        }
        assert!(today_days() > parse_days("2026-01-01").unwrap());
    }

    #[test]
    fn what_is_not_a_date_says_so() {
        assert_eq!(parse_days("2026-13-01"), Err("`2026-13-01` is not a day of the calendar".into()));
        assert_eq!(parse_days("2025-02-29"), Err("`2025-02-29` is not a day of the calendar".into()));
        assert_eq!(parse_days("1 Dec 2026"), Err("`1 Dec 2026` is not YYYY-MM-DD".into()));
        assert_eq!(parse_days("2026-1-01"), Err("`2026-1-01` is not YYYY-MM-DD".into()));
    }
}
