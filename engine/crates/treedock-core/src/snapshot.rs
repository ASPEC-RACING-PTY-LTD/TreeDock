// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! `.tdsnap` snapshot format (MASTER_PLAN §6.4).
//!
//! ```text
//! magic  "TDSNAP\0\x01"            8 bytes
//! u32    format version (1)
//! u32    section count
//! repeat:
//!   u32  section id
//!   u64  uncompressed length
//!   u64  compressed length
//!   u64  XXH3-64 of the uncompressed bytes
//!   [..] zstd-compressed bytes
//! ```
//! All integers are little-endian. Snapshots are treated as untrusted input:
//! every length, index and link is validated before a [`Tree`] is returned.

use std::io::{self, Read, Write};

use xxhash_rust::xxh3::xxh3_64;

use crate::tree::{ExtTable, NONE, Tree, TreeMeta, flags};

pub const MAGIC: &[u8; 8] = b"TDSNAP\0\x01";
pub const FORMAT_VERSION: u32 = 1;
const MAX_SECTION: u64 = 1 << 36;

#[derive(Debug)]
pub enum SnapshotError {
    Io(io::Error),
    Format(String),
}

impl std::fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SnapshotError::Io(e) => write!(f, "I/O error: {e}"),
            SnapshotError::Format(m) => write!(f, "invalid snapshot: {m}"),
        }
    }
}

impl std::error::Error for SnapshotError {}

impl From<io::Error> for SnapshotError {
    fn from(e: io::Error) -> Self {
        SnapshotError::Io(e)
    }
}

fn bad(m: impl Into<String>) -> SnapshotError {
    SnapshotError::Format(m.into())
}

trait Le: Sized + Copy {
    const N: usize;
    fn put(self, out: &mut Vec<u8>);
    fn get(b: &[u8]) -> Self;
}

macro_rules! le {
    ($($t:ty),*) => {$(
        impl Le for $t {
            const N: usize = std::mem::size_of::<$t>();
            fn put(self, out: &mut Vec<u8>) { out.extend_from_slice(&self.to_le_bytes()); }
            fn get(b: &[u8]) -> Self { <$t>::from_le_bytes(b.try_into().unwrap()) }
        }
    )*};
}
le!(u16, u32, u64, i64);

fn encode<T: Le>(v: &[T]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * T::N);
    for &x in v {
        x.put(&mut out);
    }
    out
}

fn decode<T: Le>(b: &[u8], expect: usize, what: &str) -> Result<Vec<T>, SnapshotError> {
    if b.len() != expect * T::N {
        return Err(bad(format!("{what}: expected {expect} values, found {} bytes", b.len())));
    }
    Ok(b.chunks_exact(T::N).map(T::get).collect())
}

mod id {
    pub const META: u32 = 1;
    pub const EXTS: u32 = 2;
    pub const NAMES: u32 = 3;
    pub const PARENT: u32 = 10;
    pub const FIRST_CHILD: u32 = 11;
    pub const NEXT_SIBLING: u32 = 12;
    pub const NAME_OFF: u32 = 13;
    pub const NAME_LEN: u32 = 14;
    pub const FLAGS: u32 = 15;
    pub const EXT: u32 = 16;
    pub const ATTRS: u32 = 17;
    pub const LOGICAL: u32 = 18;
    pub const ALLOCATED: u32 = 19;
    pub const MTIME: u32 = 20;
    pub const CTIME: u32 = 21;
    pub const FILE_ID: u32 = 22;
    pub const DIR_ORD: u32 = 23;
    pub const DIR_FILES: u32 = 30;
    pub const DIR_DIRS: u32 = 31;
    pub const DIR_PENDING: u32 = 32;
    pub const DIR_ERRORS: u32 = 33;
}

/// Writes `tree` to `out`. Compression level 3 balances speed and size.
pub fn write(tree: &Tree, out: &mut impl Write) -> Result<(), SnapshotError> {
    let sections: Vec<(u32, Vec<u8>)> = vec![
        (id::META, serde_json::to_vec(&tree.meta).map_err(|e| bad(e.to_string()))?),
        (id::EXTS, serde_json::to_vec(tree.exts.names()).map_err(|e| bad(e.to_string()))?),
        (id::NAMES, tree.names.clone()),
        (id::PARENT, encode(&tree.parent)),
        (id::FIRST_CHILD, encode(&tree.first_child)),
        (id::NEXT_SIBLING, encode(&tree.next_sibling)),
        (id::NAME_OFF, encode(&tree.name_off)),
        (id::NAME_LEN, encode(&tree.name_len)),
        (id::FLAGS, encode(&tree.flags)),
        (id::EXT, encode(&tree.ext)),
        (id::ATTRS, encode(&tree.attrs)),
        (id::LOGICAL, encode(&tree.logical)),
        (id::ALLOCATED, encode(&tree.allocated)),
        (id::MTIME, encode(&tree.mtime)),
        (id::CTIME, encode(&tree.ctime)),
        (id::FILE_ID, encode(&tree.file_id)),
        (id::DIR_ORD, encode(&tree.dir_ord)),
        (id::DIR_FILES, encode(&tree.dir_files)),
        (id::DIR_DIRS, encode(&tree.dir_dirs)),
        (id::DIR_PENDING, encode(&tree.dir_pending)),
        (id::DIR_ERRORS, encode(&tree.dir_errors)),
    ];
    out.write_all(MAGIC)?;
    out.write_all(&FORMAT_VERSION.to_le_bytes())?;
    out.write_all(&(sections.len() as u32).to_le_bytes())?;
    for (sid, raw) in sections {
        let comp = zstd::bulk::compress(&raw, 3)?;
        out.write_all(&sid.to_le_bytes())?;
        out.write_all(&(raw.len() as u64).to_le_bytes())?;
        out.write_all(&(comp.len() as u64).to_le_bytes())?;
        out.write_all(&xxh3_64(&raw).to_le_bytes())?;
        out.write_all(&comp)?;
    }
    out.flush()?;
    Ok(())
}

/// Writes atomically: to `<path>.tmp`, then renames over `path`.
pub fn save(tree: &Tree, path: &std::path::Path) -> Result<(), SnapshotError> {
    let tmp = path.with_extension("tdsnap.tmp");
    {
        let f = std::fs::File::create(&tmp)?;
        let mut w = io::BufWriter::new(f);
        write(tree, &mut w)?;
        w.into_inner().map_err(|e| e.into_error())?.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn load(path: &std::path::Path) -> Result<Tree, SnapshotError> {
    let f = std::fs::File::open(path)?;
    read(&mut io::BufReader::new(f))
}

fn read_u32(r: &mut impl Read) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}

fn read_u64(r: &mut impl Read) -> io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

/// Reads and fully validates a snapshot.
pub fn read(r: &mut impl Read) -> Result<Tree, SnapshotError> {
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(bad("not a TreeDock snapshot"));
    }
    let version = read_u32(r)?;
    if version != FORMAT_VERSION {
        return Err(bad(format!("unsupported format version {version}")));
    }
    let count = read_u32(r)?;
    if count > 256 {
        return Err(bad("too many sections"));
    }
    let mut sections = std::collections::HashMap::new();
    for _ in 0..count {
        let sid = read_u32(r)?;
        let raw_len = read_u64(r)?;
        let comp_len = read_u64(r)?;
        let hash = read_u64(r)?;
        if raw_len > MAX_SECTION || comp_len > MAX_SECTION {
            return Err(bad("section too large"));
        }
        let mut comp = Vec::new();
        r.take(comp_len).read_to_end(&mut comp)?;
        if comp.len() as u64 != comp_len {
            return Err(bad("truncated section"));
        }
        // Stream-decode so a forged length cannot force a huge up-front allocation.
        let mut raw = Vec::new();
        zstd::stream::read::Decoder::new(comp.as_slice())
            .and_then(|d| d.take(raw_len + 1).read_to_end(&mut raw))
            .map_err(|e| bad(format!("section {sid}: {e}")))?;
        if raw.len() as u64 != raw_len || xxh3_64(&raw) != hash {
            return Err(bad(format!("section {sid} checksum mismatch")));
        }
        sections.insert(sid, raw);
    }
    let mut take = |sid: u32| sections.remove(&sid).ok_or_else(|| bad(format!("missing section {sid}")));

    let meta: TreeMeta = serde_json::from_slice(&take(id::META)?).map_err(|e| bad(e.to_string()))?;
    let ext_names: Vec<String> = serde_json::from_slice(&take(id::EXTS)?).map_err(|e| bad(e.to_string()))?;
    if ext_names.is_empty() || ext_names.len() > u16::MAX as usize || !ext_names[0].is_empty() {
        return Err(bad("invalid extension table"));
    }
    let names = take(id::NAMES)?;
    let parent_raw = take(id::PARENT)?;
    let n = parent_raw.len() / 4;
    if n == 0 {
        return Err(bad("empty tree"));
    }
    let mut t = Tree {
        meta,
        parent: decode(&parent_raw, n, "parent")?,
        first_child: decode(&take(id::FIRST_CHILD)?, n, "first_child")?,
        next_sibling: decode(&take(id::NEXT_SIBLING)?, n, "next_sibling")?,
        name_off: decode(&take(id::NAME_OFF)?, n, "name_off")?,
        name_len: decode(&take(id::NAME_LEN)?, n, "name_len")?,
        flags: decode(&take(id::FLAGS)?, n, "flags")?,
        ext: decode(&take(id::EXT)?, n, "ext")?,
        attrs: decode(&take(id::ATTRS)?, n, "attrs")?,
        logical: decode(&take(id::LOGICAL)?, n, "logical")?,
        allocated: decode(&take(id::ALLOCATED)?, n, "allocated")?,
        mtime: decode(&take(id::MTIME)?, n, "mtime")?,
        ctime: decode(&take(id::CTIME)?, n, "ctime")?,
        file_id: decode(&take(id::FILE_ID)?, n, "file_id")?,
        dir_ord: decode(&take(id::DIR_ORD)?, n, "dir_ord")?,
        names,
        exts: ExtTable::from_names(ext_names),
        ..Default::default()
    };
    let dirs = t.dir_ord.iter().filter(|&&d| d != NONE).count();
    t.dir_files = decode(&take(id::DIR_FILES)?, dirs, "dir_files")?;
    t.dir_dirs = decode(&take(id::DIR_DIRS)?, dirs, "dir_dirs")?;
    t.dir_pending = decode(&take(id::DIR_PENDING)?, dirs, "dir_pending")?;
    t.dir_errors = decode(&take(id::DIR_ERRORS)?, dirs, "dir_errors")?;
    validate(&t)?;
    Ok(t)
}

/// Structural validation so a hostile file cannot cause panics or infinite loops.
fn validate(t: &Tree) -> Result<(), SnapshotError> {
    let n = t.parent.len();
    let n_ext = t.exts.names().len();
    let mut seen_dir = vec![false; t.dir_files.len()];
    for i in 0..n {
        let in_range = |v: u32| v == NONE || (v as usize) < n;
        if !in_range(t.parent[i]) || !in_range(t.first_child[i]) || !in_range(t.next_sibling[i]) {
            return Err(bad(format!("node {i}: link out of range")));
        }
        let end = t.name_off[i] as usize + t.name_len[i] as usize;
        if end > t.names.len() {
            return Err(bad(format!("node {i}: name out of range")));
        }
        if t.ext[i] as usize >= n_ext {
            return Err(bad(format!("node {i}: extension out of range")));
        }
        let is_dir = t.flags[i] & flags::DIR != 0;
        match (is_dir, t.dir_ord[i]) {
            (true, d) if d != NONE && (d as usize) < seen_dir.len() && !seen_dir[d as usize] => {
                seen_dir[d as usize] = true
            }
            (false, NONE) => {}
            _ => return Err(bad(format!("node {i}: inconsistent directory index"))),
        }
        if !is_dir && t.first_child[i] != NONE {
            return Err(bad(format!("node {i}: file has children")));
        }
    }
    if t.parent[0] != NONE || t.flags[0] & flags::DIR == 0 {
        return Err(bad("invalid root"));
    }
    // Every reachable node is visited exactly once and points back to its parent.
    let mut visited = vec![false; n];
    let mut stack = vec![0u32];
    visited[0] = true;
    while let Some(p) = stack.pop() {
        let mut c = t.first_child[p as usize];
        while c != NONE {
            if visited[c as usize] || t.parent[c as usize] != p {
                return Err(bad(format!("node {c}: corrupt child list")));
            }
            visited[c as usize] = true;
            stack.push(c);
            c = t.next_sibling[c as usize];
        }
    }
    t.check_invariants().map_err(bad)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::tests::sample;

    fn to_bytes(t: &Tree) -> Vec<u8> {
        let mut buf = Vec::new();
        write(t, &mut buf).unwrap();
        buf
    }

    #[test]
    fn roundtrip() {
        let mut t = sample();
        t.meta.scan_mode = "walk".into();
        t.meta.cluster_size = 4096;
        let buf = to_bytes(&t);
        let r = read(&mut buf.as_slice()).unwrap();
        assert_eq!(r.meta, t.meta);
        assert_eq!(r.len(), t.len());
        for id in 0..t.len() as u32 {
            assert_eq!(r.path(id), t.path(id));
            assert_eq!(r.allocated(id), t.allocated(id));
            assert_eq!(r.ext_name(id), t.ext_name(id));
            assert_eq!(r.dir_stats(id), t.dir_stats(id));
        }
    }

    #[test]
    fn rejects_corruption() {
        let buf = to_bytes(&sample());
        assert!(read(&mut &buf[..buf.len() - 3]).is_err());
        let mut b = buf.clone();
        b[0] = b'X';
        assert!(read(&mut b.as_slice()).is_err());
        // Flip every byte past the header one at a time: must never panic.
        for i in 16..buf.len() {
            let mut b = buf.clone();
            b[i] ^= 0xFF;
            let _ = read(&mut b.as_slice());
        }
    }

    #[test]
    fn rejects_structural_cycles() {
        let mut t = sample();
        // Make a child list loop back to the root.
        let last = t.children(Tree::ROOT).last().unwrap();
        t.next_sibling[last as usize] = t.first_child[0];
        assert!(read(&mut to_bytes(&t).as_slice()).is_err());
    }

    #[test]
    fn save_and_load_file() {
        let dir = std::env::temp_dir().join(format!("tdsnap-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("x.tdsnap");
        save(&sample(), &p).unwrap();
        let t = load(&p).unwrap();
        assert_eq!(t.logical(Tree::ROOT), 5100);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
