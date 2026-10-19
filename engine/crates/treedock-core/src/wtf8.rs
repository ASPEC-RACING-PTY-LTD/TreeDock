// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Lossless storage of Windows file names.
//!
//! Windows names are UTF-16 but may contain unpaired surrogates. We store them
//! as WTF-8 (UTF-8 generalised to allow encoded surrogates) so every name
//! round-trips exactly back to UTF-16, while valid names stay plain UTF-8.

/// Appends the WTF-8 encoding of `units` to `out`.
pub fn encode_utf16(units: &[u16], out: &mut Vec<u8>) {
    let mut i = 0;
    while i < units.len() {
        let u = units[i] as u32;
        let cp = if (0xD800..0xDC00).contains(&u)
            && i + 1 < units.len()
            && (0xDC00..0xE000).contains(&(units[i + 1] as u32))
        {
            let lo = units[i + 1] as u32;
            i += 1;
            0x10000 + ((u - 0xD800) << 10) + (lo - 0xDC00)
        } else {
            u
        };
        push_code_point(cp, out);
        i += 1;
    }
}

fn push_code_point(cp: u32, out: &mut Vec<u8>) {
    if cp < 0x80 {
        out.push(cp as u8);
    } else if cp < 0x800 {
        out.push(0xC0 | (cp >> 6) as u8);
        out.push(0x80 | (cp & 0x3F) as u8);
    } else if cp < 0x10000 {
        out.push(0xE0 | (cp >> 12) as u8);
        out.push(0x80 | ((cp >> 6) & 0x3F) as u8);
        out.push(0x80 | (cp & 0x3F) as u8);
    } else {
        out.push(0xF0 | (cp >> 18) as u8);
        out.push(0x80 | ((cp >> 12) & 0x3F) as u8);
        out.push(0x80 | ((cp >> 6) & 0x3F) as u8);
        out.push(0x80 | (cp & 0x3F) as u8);
    }
}

/// Decodes WTF-8 produced by [`encode_utf16`] back to UTF-16, appending to `out`.
/// Malformed input (which our encoder never produces) is replaced with U+FFFD.
pub fn decode_to_utf16(bytes: &[u8], out: &mut Vec<u16>) {
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        let (cp, len) = if b < 0x80 {
            (b as u32, 1)
        } else if b & 0xE0 == 0xC0 && i + 1 < bytes.len() {
            (((b as u32 & 0x1F) << 6) | (bytes[i + 1] as u32 & 0x3F), 2)
        } else if b & 0xF0 == 0xE0 && i + 2 < bytes.len() {
            (((b as u32 & 0x0F) << 12) | ((bytes[i + 1] as u32 & 0x3F) << 6) | (bytes[i + 2] as u32 & 0x3F), 3)
        } else if b & 0xF8 == 0xF0 && i + 3 < bytes.len() {
            (
                ((b as u32 & 0x07) << 18)
                    | ((bytes[i + 1] as u32 & 0x3F) << 12)
                    | ((bytes[i + 2] as u32 & 0x3F) << 6)
                    | (bytes[i + 3] as u32 & 0x3F),
                4,
            )
        } else {
            (0xFFFD, 1)
        };
        if cp >= 0x10000 {
            let v = cp - 0x10000;
            out.push(0xD800 + (v >> 10) as u16);
            out.push(0xDC00 + (v & 0x3FF) as u16);
        } else {
            out.push(cp as u16);
        }
        i += len;
    }
}

/// Converts WTF-8 to a displayable `String`, replacing lone surrogates with U+FFFD.
pub fn to_string_lossy(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_owned(),
        Err(_) => {
            let mut units = Vec::with_capacity(bytes.len());
            decode_to_utf16(bytes, &mut units);
            String::from_utf16_lossy(&units)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(units: &[u16]) {
        let mut enc = Vec::new();
        encode_utf16(units, &mut enc);
        let mut dec = Vec::new();
        decode_to_utf16(&enc, &mut dec);
        assert_eq!(dec, units);
    }

    #[test]
    fn ascii_and_unicode_roundtrip() {
        roundtrip(&"hello.txt".encode_utf16().collect::<Vec<_>>());
        roundtrip(&"Ünïcødé 名前 🎉.mp4".encode_utf16().collect::<Vec<_>>());
    }

    #[test]
    fn valid_names_are_plain_utf8() {
        let s = "Ünïcødé 名前 🎉";
        let mut enc = Vec::new();
        encode_utf16(&s.encode_utf16().collect::<Vec<_>>(), &mut enc);
        assert_eq!(enc, s.as_bytes());
    }

    #[test]
    fn lone_surrogates_roundtrip() {
        roundtrip(&[0x61, 0xD800, 0x62]);
        roundtrip(&[0xDC00]);
        roundtrip(&[0xDBFF, 0xDBFF, 0xDFFF]);
        assert_eq!(
            to_string_lossy(&{
                let mut v = Vec::new();
                encode_utf16(&[0x61, 0xD800], &mut v);
                v
            }),
            "a\u{FFFD}"
        );
    }
}
