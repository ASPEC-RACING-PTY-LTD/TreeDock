// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! NTFS file-record parsing (MASTER_PLAN §5.2).
//!
//! Original implementation based on publicly documented NTFS on-disk
//! structures. Input is untrusted: every read is bounds-checked and malformed
//! records yield an error instead of a panic.

pub const ATTR_STANDARD_INFORMATION: u32 = 0x10;
pub const ATTR_ATTRIBUTE_LIST: u32 = 0x20;
pub const ATTR_FILE_NAME: u32 = 0x30;
pub const ATTR_DATA: u32 = 0x80;
pub const ATTR_REPARSE_POINT: u32 = 0xC0;
pub const ATTR_END: u32 = 0xFFFF_FFFF;

pub const RECORD_IN_USE: u16 = 0x0001;
pub const RECORD_IS_DIRECTORY: u16 = 0x0002;

pub const ATTR_FLAG_COMPRESSED: u16 = 0x0001;
pub const ATTR_FLAG_ENCRYPTED: u16 = 0x4000;
pub const ATTR_FLAG_SPARSE: u16 = 0x8000;

/// `$FILE_NAME` namespaces.
pub const NS_POSIX: u8 = 0;
pub const NS_WIN32: u8 = 1;
pub const NS_DOS: u8 = 2;
pub const NS_WIN32_AND_DOS: u8 = 3;

/// Low 48 bits of a file reference are the record number; high 16 the sequence.
pub fn ref_record(r: u64) -> u64 {
    r & 0x0000_FFFF_FFFF_FFFF
}

pub fn ref_seq(r: u64) -> u16 {
    (r >> 48) as u16
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordError {
    BadSignature,
    BadFixup,
    Truncated,
    BadAttribute,
}

impl std::fmt::Display for RecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            RecordError::BadSignature => "bad record signature",
            RecordError::BadFixup => "update sequence mismatch (torn or corrupt record)",
            RecordError::Truncated => "record truncated",
            RecordError::BadAttribute => "malformed attribute",
        })
    }
}

impl std::error::Error for RecordError {}

#[inline]
fn u16_at(b: &[u8], o: usize) -> Result<u16, RecordError> {
    b.get(o..o + 2).map(|s| u16::from_le_bytes([s[0], s[1]])).ok_or(RecordError::Truncated)
}

#[inline]
fn u32_at(b: &[u8], o: usize) -> Result<u32, RecordError> {
    b.get(o..o + 4).map(|s| u32::from_le_bytes(s.try_into().unwrap())).ok_or(RecordError::Truncated)
}

#[inline]
fn u64_at(b: &[u8], o: usize) -> Result<u64, RecordError> {
    b.get(o..o + 8).map(|s| u64::from_le_bytes(s.try_into().unwrap())).ok_or(RecordError::Truncated)
}

fn utf16_le(b: &[u8]) -> Vec<u16> {
    b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect()
}

/// Applies the update-sequence-array fixups in place. `sector` is the
/// bytes-per-sector value (usually 512).
pub fn apply_fixups(rec: &mut [u8], sector: usize) -> Result<(), RecordError> {
    if rec.get(0..4) != Some(b"FILE") {
        return Err(RecordError::BadSignature);
    }
    let usa_off = u16_at(rec, 4)? as usize;
    let usa_count = u16_at(rec, 6)? as usize;
    if usa_count < 2 || sector < 2 || (usa_count - 1) * sector > rec.len() {
        return Err(RecordError::BadFixup);
    }
    let usn = u16_at(rec, usa_off)?;
    for i in 1..usa_count {
        let end = i * sector - 2;
        if u16_at(rec, end)? != usn {
            return Err(RecordError::BadFixup);
        }
        let fix = u16_at(rec, usa_off + 2 * i)?;
        rec[end..end + 2].copy_from_slice(&fix.to_le_bytes());
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileName {
    pub parent: u64,
    pub namespace: u8,
    pub name: Vec<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataAttr {
    /// Empty for the unnamed (main) stream.
    pub name: Vec<u16>,
    pub logical: u64,
    /// Bytes actually allocated (compressed size for compressed/sparse streams).
    pub allocated: u64,
    pub flags: u16,
}

/// A parsed record (base or extension).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedRecord {
    pub seq: u16,
    pub flags: u16,
    /// Base record reference (0 for base records).
    pub base: u64,
    pub std_attrs: Option<u32>,
    pub ctime: i64,
    pub mtime: i64,
    pub names: Vec<FileName>,
    /// Only first extents (starting VCN 0), which carry the sizes.
    pub data: Vec<DataAttr>,
    pub reparse_tag: u32,
    pub has_attribute_list: bool,
}

impl ParsedRecord {
    pub fn in_use(&self) -> bool {
        self.flags & RECORD_IN_USE != 0
    }

    pub fn is_dir(&self) -> bool {
        self.flags & RECORD_IS_DIRECTORY != 0
    }
}

/// Parses a fixed-up record.
pub fn parse_record(rec: &[u8]) -> Result<ParsedRecord, RecordError> {
    if rec.get(0..4) != Some(b"FILE") {
        return Err(RecordError::BadSignature);
    }
    let mut r =
        ParsedRecord { seq: u16_at(rec, 16)?, flags: u16_at(rec, 22)?, base: u64_at(rec, 32)?, ..Default::default() };
    let used = (u32_at(rec, 24)? as usize).min(rec.len());
    let mut off = u16_at(rec, 20)? as usize;
    while off + 8 <= used {
        let ty = u32_at(rec, off)?;
        if ty == ATTR_END {
            break;
        }
        let len = u32_at(rec, off + 4)? as usize;
        if len < 16 || off + len > used {
            return Err(RecordError::BadAttribute);
        }
        let a = &rec[off..off + len];
        let nonresident = a[8] != 0;
        let name_len = a[9] as usize;
        let name_off = u16_at(a, 10)? as usize;
        let aflags = u16_at(a, 12)?;
        let attr_name = || -> Result<Vec<u16>, RecordError> {
            let bytes = a.get(name_off..name_off + 2 * name_len).ok_or(RecordError::BadAttribute)?;
            Ok(utf16_le(bytes))
        };
        let resident_value = || -> Result<&[u8], RecordError> {
            let vlen = u32_at(a, 16)? as usize;
            let voff = u16_at(a, 20)? as usize;
            a.get(voff..voff + vlen).ok_or(RecordError::BadAttribute)
        };
        match ty {
            ATTR_STANDARD_INFORMATION if !nonresident => {
                let v = resident_value()?;
                r.ctime = u64_at(v, 0)? as i64;
                r.mtime = u64_at(v, 8)? as i64;
                r.std_attrs = Some(u32_at(v, 32)?);
            }
            ATTR_ATTRIBUTE_LIST => r.has_attribute_list = true,
            ATTR_FILE_NAME if !nonresident => {
                let v = resident_value()?;
                let n = *v.get(64).ok_or(RecordError::BadAttribute)? as usize;
                let ns = *v.get(65).ok_or(RecordError::BadAttribute)?;
                let bytes = v.get(66..66 + 2 * n).ok_or(RecordError::BadAttribute)?;
                r.names.push(FileName { parent: u64_at(v, 0)?, namespace: ns, name: utf16_le(bytes) });
            }
            ATTR_DATA => {
                if nonresident {
                    let start_vcn = u64_at(a, 16)?;
                    if start_vcn == 0 {
                        let allocated = u64_at(a, 40)?;
                        let logical = u64_at(a, 48)?;
                        let compressed = u16_at(a, 34)? != 0 || aflags & (ATTR_FLAG_COMPRESSED | ATTR_FLAG_SPARSE) != 0;
                        let on_disk = if compressed && len >= 72 { u64_at(a, 64)? } else { allocated };
                        r.data.push(DataAttr { name: attr_name()?, logical, allocated: on_disk, flags: aflags });
                    }
                } else {
                    let v = resident_value()?;
                    r.data.push(DataAttr { name: attr_name()?, logical: v.len() as u64, allocated: 0, flags: aflags });
                }
            }
            ATTR_REPARSE_POINT => {
                if !nonresident {
                    r.reparse_tag = u32_at(resident_value()?, 0)?;
                } else {
                    // Tag is in the (non-resident) value; mark as reparse with unknown tag.
                    r.reparse_tag = u32::MAX;
                }
            }
            _ => {}
        }
        off += len;
    }
    Ok(r)
}

/// One run of a non-resident attribute: `lcn` is `None` for sparse runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Run {
    pub lcn: Option<i64>,
    pub clusters: u64,
}

/// Decodes an NTFS runlist (mapping pairs).
pub fn decode_runlist(b: &[u8]) -> Result<Vec<Run>, RecordError> {
    let mut runs = Vec::new();
    let mut i = 0;
    let mut lcn: i64 = 0;
    while i < b.len() {
        let h = b[i];
        if h == 0 {
            return Ok(runs);
        }
        let (len_size, off_size) = ((h & 0x0F) as usize, (h >> 4) as usize);
        if len_size == 0 || len_size > 8 || off_size > 8 || i + 1 + len_size + off_size > b.len() {
            return Err(RecordError::BadAttribute);
        }
        let mut len: u64 = 0;
        for k in 0..len_size {
            len |= (b[i + 1 + k] as u64) << (8 * k);
        }
        let run = if off_size == 0 {
            Run { lcn: None, clusters: len }
        } else {
            let mut d: i64 = 0;
            for k in 0..off_size {
                d |= (b[i + 1 + len_size + k] as i64) << (8 * k);
            }
            // Sign-extend.
            let shift = 64 - 8 * off_size as u32;
            d = (d << shift) >> shift;
            lcn = lcn.checked_add(d).ok_or(RecordError::BadAttribute)?;
            if lcn < 0 {
                return Err(RecordError::BadAttribute);
            }
            Run { lcn: Some(lcn), clusters: len }
        };
        runs.push(run);
        i += 1 + len_size + off_size;
    }
    Err(RecordError::Truncated)
}

/// Returns the runlist of the unnamed `$DATA` attribute of a fixed-up record
/// (used to locate the extents of `$MFT` itself from record 0).
pub fn data_runlist(rec: &[u8]) -> Result<Vec<Run>, RecordError> {
    let used = (u32_at(rec, 24)? as usize).min(rec.len());
    let mut off = u16_at(rec, 20)? as usize;
    while off + 8 <= used {
        let ty = u32_at(rec, off)?;
        if ty == ATTR_END {
            break;
        }
        let len = u32_at(rec, off + 4)? as usize;
        if len < 16 || off + len > used {
            return Err(RecordError::BadAttribute);
        }
        let a = &rec[off..off + len];
        if ty == ATTR_DATA && a[8] != 0 && a[9] == 0 {
            if u64_at(a, 16)? != 0 {
                return Err(RecordError::BadAttribute);
            }
            let ro = u16_at(a, 32)? as usize;
            return decode_runlist(a.get(ro..).ok_or(RecordError::BadAttribute)?);
        }
        off += len;
    }
    Err(RecordError::BadAttribute)
}

/// Test/fixture helper that builds well-formed records.
pub mod builder {
    use super::*;

    pub struct RecordBuilder {
        attrs: Vec<u8>,
        flags: u16,
        seq: u16,
        base: u64,
        size: usize,
    }

    fn attr_header(ty: u32, total: usize, nonresident: bool, name: &[u16], flags: u16) -> Vec<u8> {
        let mut h = vec![0u8; 16];
        h[0..4].copy_from_slice(&ty.to_le_bytes());
        h[4..8].copy_from_slice(&(total as u32).to_le_bytes());
        h[8] = nonresident as u8;
        h[9] = name.len() as u8;
        h[12..14].copy_from_slice(&flags.to_le_bytes());
        h
    }

    fn pad8(v: &mut Vec<u8>) {
        while !v.len().is_multiple_of(8) {
            v.push(0);
        }
    }

    impl RecordBuilder {
        pub fn new(seq: u16, flags: u16) -> Self {
            RecordBuilder { attrs: Vec::new(), flags, seq, base: 0, size: 1024 }
        }

        pub fn base(mut self, base: u64) -> Self {
            self.base = base;
            self
        }

        pub fn size(mut self, size: usize) -> Self {
            self.size = size;
            self
        }

        fn resident(mut self, ty: u32, name: &[u16], value: &[u8], flags: u16) -> Self {
            let name_off = 24;
            let value_off = (name_off + name.len() * 2).div_ceil(8) * 8;
            let total = (value_off + value.len()).div_ceil(8) * 8;
            let mut a = attr_header(ty, total, false, name, flags);
            a[10..12].copy_from_slice(&(name_off as u16).to_le_bytes());
            a.extend_from_slice(&(value.len() as u32).to_le_bytes());
            a.extend_from_slice(&(value_off as u16).to_le_bytes());
            a.extend_from_slice(&[0, 0]);
            for c in name {
                a.extend_from_slice(&c.to_le_bytes());
            }
            a.resize(value_off, 0);
            a.extend_from_slice(value);
            pad8(&mut a);
            self.attrs.extend_from_slice(&a);
            self
        }

        pub fn std_info(self, attrs: u32, ctime: i64, mtime: i64) -> Self {
            let mut v = vec![0u8; 72];
            v[0..8].copy_from_slice(&ctime.to_le_bytes());
            v[8..16].copy_from_slice(&mtime.to_le_bytes());
            v[32..36].copy_from_slice(&attrs.to_le_bytes());
            self.resident(ATTR_STANDARD_INFORMATION, &[], &v, 0)
        }

        pub fn file_name(self, parent_ref: u64, ns: u8, name: &str) -> Self {
            let n: Vec<u16> = name.encode_utf16().collect();
            let mut v = vec![0u8; 66];
            v[0..8].copy_from_slice(&parent_ref.to_le_bytes());
            v[64] = n.len() as u8;
            v[65] = ns;
            for c in &n {
                v.extend_from_slice(&c.to_le_bytes());
            }
            self.resident(ATTR_FILE_NAME, &[], &v, 0)
        }

        pub fn data_resident(self, stream: &str, len: usize) -> Self {
            let n: Vec<u16> = stream.encode_utf16().collect();
            self.resident(ATTR_DATA, &n, &vec![0xAB; len], 0)
        }

        pub fn reparse(self, tag: u32) -> Self {
            let mut v = tag.to_le_bytes().to_vec();
            v.extend_from_slice(&[0; 4]);
            self.resident(ATTR_REPARSE_POINT, &[], &v, 0)
        }

        pub fn attribute_list(self) -> Self {
            self.resident(ATTR_ATTRIBUTE_LIST, &[], &[0; 32], 0)
        }

        /// Non-resident `$DATA`. `compressed` = Some(on_disk) adds a compressed-size field.
        pub fn data_nonresident(
            mut self,
            stream: &str,
            start_vcn: u64,
            allocated: u64,
            logical: u64,
            compressed: Option<u64>,
            runlist: &[u8],
        ) -> Self {
            let n: Vec<u16> = stream.encode_utf16().collect();
            let header_len = if compressed.is_some() { 72 } else { 64 };
            let name_off = header_len;
            let run_off = (name_off + n.len() * 2).div_ceil(8) * 8;
            let total = (run_off + runlist.len()).div_ceil(8) * 8;
            let flags = if compressed.is_some() { ATTR_FLAG_COMPRESSED } else { 0 };
            let mut a = attr_header(ATTR_DATA, total, true, &n, flags);
            a[10..12].copy_from_slice(&(name_off as u16).to_le_bytes());
            a.extend_from_slice(&start_vcn.to_le_bytes()); // 16
            a.extend_from_slice(&0u64.to_le_bytes()); // 24 last vcn
            a.extend_from_slice(&(run_off as u16).to_le_bytes()); // 32
            a.extend_from_slice(&(if compressed.is_some() { 4u16 } else { 0 }).to_le_bytes()); // 34
            a.extend_from_slice(&[0; 4]); // 36
            a.extend_from_slice(&allocated.to_le_bytes()); // 40
            a.extend_from_slice(&logical.to_le_bytes()); // 48
            a.extend_from_slice(&logical.to_le_bytes()); // 56
            if let Some(c) = compressed {
                a.extend_from_slice(&c.to_le_bytes()); // 64
            }
            for c in &n {
                a.extend_from_slice(&c.to_le_bytes());
            }
            a.resize(run_off, 0);
            a.extend_from_slice(runlist);
            a.resize(total, 0);
            self.attrs.extend_from_slice(&a);
            self
        }

        /// Produces the on-disk bytes (with update-sequence fixups applied, as stored).
        pub fn build(self) -> Vec<u8> {
            let sector = 512;
            let sectors = self.size / sector;
            let usa_off = 48usize;
            let first_attr = (usa_off + 2 * (sectors + 1)).div_ceil(8) * 8;
            let mut r = vec![0u8; self.size];
            r[0..4].copy_from_slice(b"FILE");
            r[4..6].copy_from_slice(&(usa_off as u16).to_le_bytes());
            r[6..8].copy_from_slice(&((sectors + 1) as u16).to_le_bytes());
            r[16..18].copy_from_slice(&self.seq.to_le_bytes());
            r[20..22].copy_from_slice(&(first_attr as u16).to_le_bytes());
            r[22..24].copy_from_slice(&self.flags.to_le_bytes());
            let end = first_attr + self.attrs.len();
            assert!(end + 8 <= self.size - 2, "record too small for attributes");
            r[first_attr..end].copy_from_slice(&self.attrs);
            r[end..end + 4].copy_from_slice(&ATTR_END.to_le_bytes());
            r[24..28].copy_from_slice(&((end + 8) as u32).to_le_bytes());
            r[28..32].copy_from_slice(&(self.size as u32).to_le_bytes());
            r[32..40].copy_from_slice(&self.base.to_le_bytes());
            // Update sequence: stash the real sector-end bytes, write the USN there.
            let usn: u16 = 0x0007;
            r[usa_off..usa_off + 2].copy_from_slice(&usn.to_le_bytes());
            for i in 1..=sectors {
                let e = i * sector - 2;
                let orig = [r[e], r[e + 1]];
                r[usa_off + 2 * i..usa_off + 2 * i + 2].copy_from_slice(&orig);
                r[e..e + 2].copy_from_slice(&usn.to_le_bytes());
            }
            r
        }
    }
}

#[cfg(test)]
mod tests {
    use super::builder::RecordBuilder;
    use super::*;

    fn parse(mut b: Vec<u8>) -> ParsedRecord {
        apply_fixups(&mut b, 512).unwrap();
        parse_record(&b).unwrap()
    }

    #[test]
    fn parses_file_with_names_and_streams() {
        let rec = RecordBuilder::new(3, RECORD_IN_USE)
            .std_info(0x20, 111, 222)
            .file_name((7u64 << 48) | 5, NS_DOS, "LONGNA~1.TXT")
            .file_name((7u64 << 48) | 5, NS_WIN32, "long name.txt")
            .data_nonresident("", 0, 8192, 5000, None, &[0x11, 0x02, 0x10, 0x00])
            .data_resident("Zone.Identifier", 26)
            .build();
        let r = parse(rec);
        assert!(r.in_use() && !r.is_dir());
        assert_eq!((r.seq, r.ctime, r.mtime, r.std_attrs), (3, 111, 222, Some(0x20)));
        assert_eq!(r.names.len(), 2);
        assert_eq!(String::from_utf16(&r.names[1].name).unwrap(), "long name.txt");
        assert_eq!(ref_record(r.names[1].parent), 5);
        assert_eq!(ref_seq(r.names[1].parent), 7);
        assert_eq!(r.data[0], DataAttr { name: vec![], logical: 5000, allocated: 8192, flags: 0 });
        assert_eq!(r.data[1].logical, 26);
        assert_eq!(r.data[1].allocated, 0);
    }

    #[test]
    fn compressed_uses_compressed_size_and_skips_later_extents() {
        let r = parse(
            RecordBuilder::new(1, RECORD_IN_USE)
                .data_nonresident("", 0, 65536, 60000, Some(12288), &[0x11, 0x10, 0x20, 0x00])
                .build(),
        );
        assert_eq!(r.data[0].allocated, 12288);
        let ext = parse(
            RecordBuilder::new(1, RECORD_IN_USE)
                .base(42 | (1u64 << 48))
                .data_nonresident("", 16, 0, 0, None, &[0x11, 0x01, 0x30, 0x00])
                .build(),
        );
        assert!(ext.data.is_empty());
        assert_eq!(ref_record(ext.base), 42);
    }

    #[test]
    fn reparse_and_dir_flags() {
        let r = parse(
            RecordBuilder::new(1, RECORD_IN_USE | RECORD_IS_DIRECTORY).reparse(0xA000_0003).attribute_list().build(),
        );
        assert!(r.is_dir());
        assert_eq!(r.reparse_tag, 0xA000_0003);
        assert!(r.has_attribute_list);
    }

    #[test]
    fn fixups_detect_torn_records() {
        let mut b = RecordBuilder::new(1, RECORD_IN_USE).std_info(0, 0, 0).build();
        b[510] ^= 0xFF;
        assert_eq!(apply_fixups(&mut b, 512), Err(RecordError::BadFixup));
        let mut b = RecordBuilder::new(1, RECORD_IN_USE).build();
        b[0] = b'B';
        assert_eq!(apply_fixups(&mut b, 512), Err(RecordError::BadSignature));
        // 4 KiB records (8 sectors).
        let r = parse(RecordBuilder::new(9, RECORD_IN_USE).size(4096).file_name(5, NS_WIN32, "x").build());
        assert_eq!(r.names.len(), 1);
    }

    #[test]
    fn runlist_decoding() {
        // 0x21: 1-byte length, 2-byte offset. Runs: 0x18 clusters @ 0x5634, then
        // 0x10 clusters @ -0x0100 relative, then sparse 0x20 clusters.
        let rl = [0x21, 0x18, 0x34, 0x56, 0x21, 0x10, 0x00, 0xFF, 0x01, 0x20, 0x00];
        let runs = decode_runlist(&rl).unwrap();
        assert_eq!(
            runs,
            vec![
                Run { lcn: Some(0x5634), clusters: 0x18 },
                Run { lcn: Some(0x5534), clusters: 0x10 },
                Run { lcn: None, clusters: 0x20 },
            ]
        );
        assert!(decode_runlist(&[0x21, 0x18]).is_err());
        assert!(decode_runlist(&[0x11, 0x01, 0x80, 0x00]).is_err()); // negative LCN
        let rec = {
            let mut b = RecordBuilder::new(1, RECORD_IN_USE).data_nonresident("", 0, 4096, 4096, None, &rl).build();
            apply_fixups(&mut b, 512).unwrap();
            b
        };
        assert_eq!(data_runlist(&rec).unwrap().len(), 3);
    }

    #[test]
    fn never_panics_on_garbage() {
        let good = RecordBuilder::new(1, RECORD_IN_USE)
            .std_info(1, 2, 3)
            .file_name(5, NS_WIN32, "file.bin")
            .data_nonresident("", 0, 4096, 100, None, &[0x11, 0x01, 0x10, 0x00])
            .build();
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..20_000 {
            let mut b = good.clone();
            for _ in 0..4 {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let i = (state % b.len() as u64) as usize;
                b[i] = (state >> 32) as u8;
            }
            if apply_fixups(&mut b, 512).is_ok() {
                let _ = parse_record(&b);
                let _ = data_runlist(&b);
            }
            let _ = parse_record(&b);
        }
    }
}
