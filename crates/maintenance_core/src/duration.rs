//! A threshold's length, in the words time_kit uses.
//!
//! time_kit (the kit's `contracts/time_kit`) counts time in slots of `SLOT_MS` = 400 ms, the chain's target slot,
//! and is private to the kit, so its slot is declared here once in the same shape. A threshold is written as whole
//! numbers with units, `500ms`, `2s`, `1m`, `3slots`, or several added together, `1m30s`, and held as milliseconds.

/// A slot's length, the chain's target slot: time_kit's `SLOT_MS`, written once here.
pub const SLOT_MS: u64 = 400;

/// The units a threshold is written in, and how many milliseconds each is.
pub const UNITS: &[(&str, u64)] = &[("ms", 1), ("slots", SLOT_MS), ("slot", SLOT_MS), ("s", 1_000), ("m", 60_000)];

/// `"1m30s"` (or `"1m 30s"`) as milliseconds, or why it does not read.
pub fn parse_ms(text: &str) -> Result<u64, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("empty".into());
    }
    let compact: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let mut rest = compact.as_str();
    let mut total: u64 = 0;
    while !rest.is_empty() {
        let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        if digits == 0 {
            return Err(format!("`{text}`: a number comes before each unit"));
        }
        let n: u64 = rest[..digits].parse().map_err(|_| format!("`{text}`: the number is too large"))?;
        rest = &rest[digits..];
        let unit_len = rest.find(|c: char| c.is_ascii_digit()).unwrap_or(rest.len());
        let unit = &rest[..unit_len];
        let per = UNITS.iter().find(|(name, _)| *name == unit).map(|(_, per)| *per).ok_or_else(|| {
            if unit.is_empty() {
                format!("`{text}`: `{n}` has no unit")
            } else {
                format!("`{text}`: `{unit}` is not a unit (ms, s, m, slots)")
            }
        })?;
        total = n.checked_mul(per).and_then(|ms| total.checked_add(ms)).ok_or_else(|| format!("`{text}` is too long"))?;
        rest = &rest[unit_len..];
    }
    Ok(total)
}

/// Milliseconds as a threshold is written: `1m30s`, `500ms`, `2s`, `0ms`.
pub fn show_ms(ms: u64) -> String {
    if ms == 0 {
        return "0ms".into();
    }
    let (m, s, rest) = (ms / 60_000, ms % 60_000 / 1_000, ms % 1_000);
    let mut out = String::new();
    if m > 0 {
        out.push_str(&format!("{m}m"));
    }
    if s > 0 {
        out.push_str(&format!("{s}s"));
    }
    if rest > 0 {
        out.push_str(&format!("{rest}ms"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_unit_reads_and_adds() {
        assert_eq!(parse_ms("500ms"), Ok(500));
        assert_eq!(parse_ms("2s"), Ok(2_000));
        assert_eq!(parse_ms("1m"), Ok(60_000));
        assert_eq!(parse_ms("3slots"), Ok(1_200));
        assert_eq!(parse_ms("1slot"), Ok(SLOT_MS));
        assert_eq!(parse_ms("1m30s"), Ok(90_000));
        assert_eq!(parse_ms(" 1s500ms "), Ok(1_500));
        assert_eq!(parse_ms("1m 30s"), Ok(90_000));
    }

    #[test]
    fn what_does_not_read_says_why() {
        assert_eq!(parse_ms(""), Err("empty".into()));
        assert_eq!(parse_ms("2"), Err("`2`: `2` has no unit".into()));
        assert_eq!(parse_ms("2h"), Err("`2h`: `h` is not a unit (ms, s, m, slots)".into()));
        assert_eq!(parse_ms("2 hours"), Err("`2 hours`: `hours` is not a unit (ms, s, m, slots)".into()));
        assert_eq!(parse_ms("s"), Err("`s`: a number comes before each unit".into()));
        assert_eq!(parse_ms("1.5s"), Err("`1.5s`: `.` is not a unit (ms, s, m, slots)".into()));
        assert!(parse_ms("99999999999999999999ms").is_err());
        assert!(parse_ms("999999999999999m").is_err());
    }

    #[test]
    fn shown_as_written() {
        for (ms, text) in [(0, "0ms"), (500, "500ms"), (2_000, "2s"), (90_000, "1m30s"), (61_001, "1m1s1ms")] {
            assert_eq!(show_ms(ms), text);
            assert_eq!(parse_ms(text), Ok(ms));
        }
    }
}
