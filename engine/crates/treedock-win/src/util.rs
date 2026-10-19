// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! UTF-16 path helpers.

pub const BACKSLASH: u16 = b'\\' as u16;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

pub fn wide_nul(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn with_nul(v: &[u16]) -> Vec<u16> {
    let mut o = Vec::with_capacity(v.len() + 1);
    o.extend_from_slice(v);
    o.push(0);
    o
}

pub fn from_wide(v: &[u16]) -> String {
    let end = v.iter().position(|&c| c == 0).unwrap_or(v.len());
    String::from_utf16_lossy(&v[..end])
}

fn starts_with(v: &[u16], p: &str) -> bool {
    let p = wide(p);
    v.len() >= p.len()
        && v[..p.len()]
            .iter()
            .zip(&p)
            .all(|(a, b)| (*a as u8 as char).eq_ignore_ascii_case(&(*b as u8 as char)) && *a < 128)
}

/// Converts a fully qualified path to its extended-length (`\\?\`) form.
pub fn to_extended(full: &[u16]) -> Vec<u16> {
    if starts_with(full, r"\\?\") || starts_with(full, r"\\.\") {
        return full.to_vec();
    }
    if starts_with(full, r"\\") {
        let mut o = wide(r"\\?\UNC\");
        o.extend_from_slice(&full[2..]);
        return o;
    }
    let mut o = wide(r"\\?\");
    o.extend_from_slice(full);
    o
}

/// Strips the extended-length prefix for display.
pub fn to_display(ext: &[u16]) -> String {
    if starts_with(ext, r"\\?\UNC\") {
        let mut o = wide(r"\\");
        o.extend_from_slice(&ext[8..]);
        return String::from_utf16_lossy(&o);
    }
    if starts_with(ext, r"\\?\") {
        return String::from_utf16_lossy(&ext[4..]);
    }
    String::from_utf16_lossy(ext)
}

pub fn join(parent: &[u16], name: &[u16]) -> Vec<u16> {
    let mut o = Vec::with_capacity(parent.len() + 1 + name.len());
    o.extend_from_slice(parent);
    if o.last() != Some(&BACKSLASH) {
        o.push(BACKSLASH);
    }
    o.extend_from_slice(name);
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_paths() {
        assert_eq!(to_display(&to_extended(&wide(r"C:\Users"))), r"C:\Users");
        assert_eq!(from_wide(&to_extended(&wide(r"C:\Users"))), r"\\?\C:\Users");
        assert_eq!(from_wide(&to_extended(&wide(r"\\srv\share\x"))), r"\\?\UNC\srv\share\x");
        assert_eq!(to_display(&to_extended(&wide(r"\\srv\share\x"))), r"\\srv\share\x");
        assert_eq!(from_wide(&to_extended(&wide(r"\\?\C:\a"))), r"\\?\C:\a");
        assert_eq!(from_wide(&join(&wide(r"C:\"), &wide("a"))), r"C:\a");
        assert_eq!(from_wide(&join(&wide(r"C:\x"), &wide("a"))), r"C:\x\a");
    }
}
