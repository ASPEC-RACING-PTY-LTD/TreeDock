// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! FILETIME helpers (100 ns ticks since 1601-01-01 UTC).

/// Ticks between 1601-01-01 and 1970-01-01.
pub const UNIX_EPOCH_TICKS: i64 = 116_444_736_000_000_000;

pub fn filetime_to_unix_ms(ft: i64) -> i64 {
    (ft - UNIX_EPOCH_TICKS).div_euclid(10_000)
}

pub fn unix_ms_to_filetime(ms: i64) -> i64 {
    ms * 10_000 + UNIX_EPOCH_TICKS
}

pub fn now_unix_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Formats a FILETIME as ISO-8601 UTC (`2024-05-01T12:34:56Z`). Zero → empty string.
pub fn filetime_to_iso(ft: i64) -> String {
    if ft <= 0 {
        return String::new();
    }
    let secs = filetime_to_unix_ms(ft).div_euclid(1000);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, (rem / 60) % 60, rem % 60)
}

/// Parses `YYYY-MM-DD` (UTC midnight) into a FILETIME.
pub fn parse_date(s: &str) -> Option<i64> {
    let mut it = s.trim().splitn(3, '-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: u32 = it.next()?.parse().ok()?;
    let d: u32 = it.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(unix_ms_to_filetime(days_from_civil(y, m, d) * 86_400_000))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_roundtrip() {
        assert_eq!(filetime_to_iso(UNIX_EPOCH_TICKS), "1970-01-01T00:00:00Z");
        let ft = parse_date("2024-02-29").unwrap();
        assert_eq!(filetime_to_iso(ft), "2024-02-29T00:00:00Z");
        assert_eq!(filetime_to_iso(ft + 10_000_000 * 3661), "2024-02-29T01:01:01Z");
        assert_eq!(filetime_to_iso(0), "");
        assert_eq!(parse_date("2024-13-01"), None);
        assert_eq!(filetime_to_iso(parse_date("1601-01-02").unwrap()), "1601-01-02T00:00:00Z");
    }
}
