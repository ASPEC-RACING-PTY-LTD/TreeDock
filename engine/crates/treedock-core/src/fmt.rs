// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Human-readable formatting.

/// Formats bytes with binary units (KiB, MiB, …), one decimal above KiB.
pub fn size_binary(b: u64) -> String {
    const U: [&str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];
    if b < 1024 {
        return format!("{b} B");
    }
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{v:.1} {}", U[i])
}

/// Formats bytes with decimal units (kB, MB, …).
pub fn size_decimal(b: u64) -> String {
    const U: [&str; 7] = ["B", "kB", "MB", "GB", "TB", "PB", "EB"];
    if b < 1000 {
        return format!("{b} B");
    }
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1000.0 && i < U.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    format!("{v:.1} {}", U[i])
}

/// Parses sizes such as `1500`, `10KB`, `1.5 GiB`, `2g`. Bare suffixes K/M/G/T are binary.
pub fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    let split = s.find(|c: char| !(c.is_ascii_digit() || c == '.')).unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let v: f64 = num.parse().ok()?;
    let mult: f64 = match unit.trim().to_ascii_lowercase().as_str() {
        "" | "b" => 1.0,
        "k" | "kib" => 1024.0,
        "kb" => 1e3,
        "m" | "mib" => 1024f64.powi(2),
        "mb" => 1e6,
        "g" | "gib" => 1024f64.powi(3),
        "gb" => 1e9,
        "t" | "tib" => 1024f64.powi(4),
        "tb" => 1e12,
        _ => return None,
    };
    let r = v * mult;
    (r.is_finite() && r >= 0.0 && r < u64::MAX as f64).then_some(r as u64)
}

/// Thousands separators for counts.
pub fn count(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting() {
        assert_eq!(size_binary(512), "512 B");
        assert_eq!(size_binary(1536), "1.5 KiB");
        assert_eq!(size_binary(5 * 1024 * 1024 * 1024), "5.0 GiB");
        assert_eq!(size_decimal(1_500_000), "1.5 MB");
        assert_eq!(count(1234567), "1,234,567");
        assert_eq!(count(12), "12");
        assert_eq!(parse_size("1.5GiB"), Some(1_610_612_736));
        assert_eq!(parse_size("10 KB"), Some(10_000));
        assert_eq!(parse_size("2g"), Some(2 * 1024 * 1024 * 1024));
        assert_eq!(parse_size("77"), Some(77));
        assert_eq!(parse_size("x"), None);
        assert_eq!(parse_size("5 parsecs"), None);
    }
}
