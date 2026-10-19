// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Compact struct-of-arrays tree store (MASTER_PLAN §6.1).
//!
//! Nodes are addressed by a `u32` [`NodeId`]. Directories carry subtree
//! aggregates; files carry their own sizes. Children form an intrusive
//! singly linked list (`first_child` / `next_sibling`), which keeps insertion
//! and removal cheap during scanning; sorted orders are computed on demand.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::wtf8;

pub type NodeId = u32;
pub const NONE: NodeId = u32::MAX;

/// Per-node flag bits.
pub mod flags {
    pub const DIR: u16 = 1 << 0;
    /// Has a reparse point of any kind.
    pub const REPARSE: u16 = 1 << 1;
    /// Name-surrogate reparse point (junction, symlink, mount point) that was not followed.
    pub const SURROGATE: u16 = 1 << 2;
    /// File has more than one hard link.
    pub const HARDLINK: u16 = 1 << 3;
    pub const COMPRESSED: u16 = 1 << 4;
    pub const SPARSE: u16 = 1 << 5;
    /// Cloud placeholder (not fully local); never opened by TreeDock.
    pub const CLOUD: u16 = 1 << 6;
    /// Directory could not be (fully) enumerated.
    pub const ERROR: u16 = 1 << 7;
    /// Directory not yet enumerated.
    pub const PENDING: u16 = 1 << 8;
    /// Allocated size is an estimate (filesystem did not report it).
    pub const ESTIMATED: u16 = 1 << 9;
    /// Removed from the tree (unlinked, kept only for id stability).
    pub const DELETED: u16 = 1 << 10;
    pub const OFFLINE: u16 = 1 << 11;
}

/// Win32 `FILE_ATTRIBUTE_*` values used by the engine.
pub mod attr {
    pub const READONLY: u32 = 0x1;
    pub const HIDDEN: u32 = 0x2;
    pub const SYSTEM: u32 = 0x4;
    pub const DIRECTORY: u32 = 0x10;
    pub const ARCHIVE: u32 = 0x20;
    pub const SPARSE_FILE: u32 = 0x200;
    pub const REPARSE_POINT: u32 = 0x400;
    pub const COMPRESSED: u32 = 0x800;
    pub const OFFLINE: u32 = 0x1000;
    pub const ENCRYPTED: u32 = 0x4000;
    pub const RECALL_ON_OPEN: u32 = 0x40000;
    pub const RECALL_ON_DATA_ACCESS: u32 = 0x400000;
}

/// Reparse-tag helpers (see Microsoft "Reparse Point Tags").
pub mod reparse {
    pub const MOUNT_POINT: u32 = 0xA000_0003;
    pub const SYMLINK: u32 = 0xA000_000C;
    pub const CLOUD: u32 = 0x9000_001A;
    pub const CLOUD_MASK: u32 = 0x0000_F000;
    pub const DEDUP: u32 = 0x8000_0013;
    pub const WOF: u32 = 0x8000_0017;

    /// Name surrogates point at another name in the namespace (junctions, symlinks…).
    pub fn is_name_surrogate(tag: u32) -> bool {
        tag & 0x2000_0000 != 0
    }

    pub fn is_cloud(tag: u32) -> bool {
        tag & !CLOUD_MASK == CLOUD
    }
}

/// A node description used when inserting.
#[derive(Clone, Debug, Default)]
pub struct NewNode<'a> {
    pub name: &'a [u16],
    pub attrs: u32,
    pub logical: u64,
    pub allocated: u64,
    pub mtime: i64,
    pub ctime: i64,
    pub file_id: u64,
    pub flags: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeError {
    CapacityExceeded,
}

impl std::error::Error for TreeError {}

impl std::fmt::Display for TreeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TreeError::CapacityExceeded => {
                write!(f, "tree capacity exceeded (more than ~4 billion nodes or 4 GiB of names)")
            }
        }
    }
}

/// Descriptive metadata about a scan.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct TreeMeta {
    /// Root path as displayed (e.g. `C:\` or `\\server\share\dir`).
    pub root_path: String,
    pub filesystem: String,
    pub volume_label: String,
    pub volume_serial: u32,
    pub cluster_size: u32,
    pub volume_total: u64,
    pub volume_free: u64,
    /// Scanner that produced the tree (`walk`, `mft`, …).
    pub scan_mode: String,
    /// Unix milliseconds.
    pub started_ms: i64,
    pub finished_ms: i64,
    /// False if the scan was cancelled or never finished.
    pub complete: bool,
    pub elevated: bool,
    pub notes: Vec<String>,
}

/// Extension interner: id 0 is "no extension".
#[derive(Clone, Debug, Default)]
pub struct ExtTable {
    names: Vec<String>,
    index: HashMap<String, u16>,
}

impl ExtTable {
    pub fn new() -> Self {
        let mut t = ExtTable::default();
        t.names.push(String::new());
        t.index.insert(String::new(), 0);
        t
    }

    pub fn from_names(names: Vec<String>) -> Self {
        let index = names.iter().enumerate().map(|(i, n)| (n.clone(), i as u16)).collect();
        ExtTable { names, index }
    }

    /// Interns a lowercase extension. When the table is full, returns id 0.
    pub fn intern(&mut self, ext: &str) -> u16 {
        if let Some(&id) = self.index.get(ext) {
            return id;
        }
        if self.names.len() >= u16::MAX as usize {
            return 0;
        }
        let id = self.names.len() as u16;
        self.names.push(ext.to_owned());
        self.index.insert(ext.to_owned(), id);
        id
    }

    pub fn name(&self, id: u16) -> &str {
        self.names.get(id as usize).map(String::as_str).unwrap_or("")
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn get(&self, ext: &str) -> Option<u16> {
        self.index.get(ext).copied()
    }
}

/// Extracts the lowercase extension (without the dot) from a UTF-16 name.
/// Names whose only dot is the first character (".gitignore") have no extension.
pub fn extension_of(name: &[u16]) -> Option<String> {
    let dot = name.iter().rposition(|&c| c == b'.' as u16)?;
    if dot == 0 || dot + 1 == name.len() {
        return None;
    }
    let ext = &name[dot + 1..];
    if ext.len() > 32 {
        return None;
    }
    Some(String::from_utf16_lossy(ext).to_lowercase())
}

/// The tree store.
#[derive(Clone, Debug, Default)]
pub struct Tree {
    pub meta: TreeMeta,
    pub(crate) parent: Vec<NodeId>,
    pub(crate) first_child: Vec<NodeId>,
    pub(crate) next_sibling: Vec<NodeId>,
    pub(crate) name_off: Vec<u32>,
    pub(crate) name_len: Vec<u16>,
    pub(crate) flags: Vec<u16>,
    pub(crate) ext: Vec<u16>,
    pub(crate) attrs: Vec<u32>,
    pub(crate) logical: Vec<u64>,
    pub(crate) allocated: Vec<u64>,
    pub(crate) mtime: Vec<i64>,
    pub(crate) ctime: Vec<i64>,
    pub(crate) file_id: Vec<u64>,
    /// Index into the `dir_*` arrays, or `NONE` for files.
    pub(crate) dir_ord: Vec<u32>,
    pub(crate) dir_files: Vec<u64>,
    pub(crate) dir_dirs: Vec<u64>,
    /// Number of pending (not yet enumerated) directories in the subtree, including self.
    pub(crate) dir_pending: Vec<u32>,
    /// Number of directories with enumeration errors in the subtree, including self.
    pub(crate) dir_errors: Vec<u32>,
    pub(crate) names: Vec<u8>,
    pub(crate) exts: ExtTable,
}

/// Directory statistics for a node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirStats {
    pub files: u64,
    pub dirs: u64,
    pub pending: u32,
    pub errors: u32,
}

impl Tree {
    /// Creates a tree with a single pending root directory.
    pub fn new(root_path: &str) -> Self {
        let mut t = Tree { exts: ExtTable::new(), ..Default::default() };
        t.meta.root_path = root_path.to_owned();
        let name: Vec<u16> = root_path.encode_utf16().collect();
        t.push_node(
            NONE,
            &NewNode { name: &name, attrs: attr::DIRECTORY, flags: flags::DIR | flags::PENDING, ..Default::default() },
        )
        .expect("empty tree has capacity");
        t
    }

    pub const ROOT: NodeId = 0;

    pub fn len(&self) -> usize {
        self.parent.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parent.is_empty()
    }

    fn push_node(&mut self, parent: NodeId, n: &NewNode) -> Result<NodeId, TreeError> {
        let id = self.parent.len();
        if id >= (NONE - 1) as usize {
            return Err(TreeError::CapacityExceeded);
        }
        let off = self.names.len();
        wtf8::encode_utf16(n.name, &mut self.names);
        let len = self.names.len() - off;
        if self.names.len() > u32::MAX as usize || len > u16::MAX as usize {
            self.names.truncate(off);
            return Err(TreeError::CapacityExceeded);
        }
        let is_dir = n.flags & flags::DIR != 0;
        let ext = if is_dir { 0 } else { extension_of(n.name).map(|e| self.exts.intern(&e)).unwrap_or(0) };
        self.parent.push(parent);
        self.first_child.push(NONE);
        self.next_sibling.push(NONE);
        self.name_off.push(off as u32);
        self.name_len.push(len as u16);
        self.flags.push(n.flags);
        self.ext.push(ext);
        self.attrs.push(n.attrs);
        self.logical.push(n.logical);
        self.allocated.push(n.allocated);
        self.mtime.push(n.mtime);
        self.ctime.push(n.ctime);
        self.file_id.push(n.file_id);
        if is_dir {
            self.dir_ord.push(self.dir_files.len() as u32);
            self.dir_files.push(0);
            self.dir_dirs.push(0);
            self.dir_pending.push(if n.flags & flags::PENDING != 0 { 1 } else { 0 });
            self.dir_errors.push(0);
        } else {
            self.dir_ord.push(NONE);
        }
        let id = id as NodeId;
        if parent != NONE {
            self.next_sibling[id as usize] = self.first_child[parent as usize];
            self.first_child[parent as usize] = id;
        }
        Ok(id)
    }

    /// Adds the entries of an enumerated directory under `parent` and updates
    /// ancestor aggregates. Returns the ids of new directories that are
    /// `PENDING` (i.e. should be enumerated next).
    pub fn add_children(&mut self, parent: NodeId, entries: &[NewNode]) -> Result<Vec<NodeId>, TreeError> {
        let mut logical = 0u64;
        let mut allocated = 0u64;
        let mut files = 0u64;
        let mut dirs = 0u64;
        let mut pending = 0u32;
        let mut new_dirs = Vec::new();
        for e in entries {
            let id = self.push_node(parent, e)?;
            if e.flags & flags::DIR != 0 {
                dirs += 1;
                if e.flags & flags::PENDING != 0 {
                    pending += 1;
                    new_dirs.push(id);
                }
            } else {
                files += 1;
                logical += e.logical;
                allocated += e.allocated;
            }
        }
        let mut cur = parent;
        while cur != NONE {
            let i = cur as usize;
            self.logical[i] += logical;
            self.allocated[i] += allocated;
            let d = self.dir_ord[i] as usize;
            self.dir_files[d] += files;
            self.dir_dirs[d] += dirs;
            self.dir_pending[d] += pending;
            cur = self.parent[i];
        }
        Ok(new_dirs)
    }

    /// Marks a pending directory as enumerated (successfully or not).
    pub fn finish_dir(&mut self, dir: NodeId, error: bool) {
        let i = dir as usize;
        if self.flags[i] & flags::PENDING == 0 {
            return;
        }
        self.flags[i] &= !flags::PENDING;
        if error {
            self.flags[i] |= flags::ERROR;
        }
        let mut cur = dir;
        while cur != NONE {
            let d = self.dir_ord[cur as usize] as usize;
            self.dir_pending[d] -= 1;
            if error {
                self.dir_errors[d] += 1;
            }
            cur = self.parent[cur as usize];
        }
    }

    /// Unlinks a node (and implicitly its subtree), subtracting its aggregates from ancestors.
    pub fn remove(&mut self, id: NodeId) {
        let i = id as usize;
        if id == Self::ROOT || self.flags[i] & flags::DELETED != 0 {
            return;
        }
        let (files, dirs, pending, errors) = match self.dir_stats(id) {
            Some(s) => (s.files, s.dirs + 1, s.pending, s.errors),
            None => (1, 0, 0, 0),
        };
        let (logical, allocated) = (self.logical[i], self.allocated[i]);
        let parent = self.parent[i];
        // Unlink from the sibling list.
        let mut prev = NONE;
        let mut cur = self.first_child[parent as usize];
        while cur != NONE && cur != id {
            prev = cur;
            cur = self.next_sibling[cur as usize];
        }
        if cur == id {
            let next = self.next_sibling[i];
            if prev == NONE {
                self.first_child[parent as usize] = next;
            } else {
                self.next_sibling[prev as usize] = next;
            }
        }
        self.flags[i] |= flags::DELETED;
        let mut a = parent;
        while a != NONE {
            let ai = a as usize;
            self.logical[ai] -= logical;
            self.allocated[ai] -= allocated;
            let d = self.dir_ord[ai] as usize;
            self.dir_files[d] -= files;
            self.dir_dirs[d] -= dirs;
            self.dir_pending[d] -= pending;
            self.dir_errors[d] -= errors;
            a = self.parent[ai];
        }
    }

    /// Sets the root directory's timestamps (scanners that know them).
    pub fn set_root_times(&mut self, mtime: i64, ctime: i64) {
        self.mtime[0] = mtime;
        self.ctime[0] = ctime;
    }

    // ---- accessors ----

    pub fn parent(&self, id: NodeId) -> NodeId {
        self.parent[id as usize]
    }

    pub fn children(&self, id: NodeId) -> Children<'_> {
        Children { tree: self, next: self.first_child[id as usize] }
    }

    pub fn child_count(&self, id: NodeId) -> usize {
        self.children(id).count()
    }

    pub fn has_children(&self, id: NodeId) -> bool {
        self.first_child[id as usize] != NONE
    }

    pub fn name_bytes(&self, id: NodeId) -> &[u8] {
        let off = self.name_off[id as usize] as usize;
        &self.names[off..off + self.name_len[id as usize] as usize]
    }

    pub fn name(&self, id: NodeId) -> String {
        wtf8::to_string_lossy(self.name_bytes(id))
    }

    pub fn name_utf16(&self, id: NodeId, out: &mut Vec<u16>) {
        wtf8::decode_to_utf16(self.name_bytes(id), out);
    }

    pub fn flags(&self, id: NodeId) -> u16 {
        self.flags[id as usize]
    }

    pub fn is_dir(&self, id: NodeId) -> bool {
        self.flags[id as usize] & flags::DIR != 0
    }

    pub fn attrs(&self, id: NodeId) -> u32 {
        self.attrs[id as usize]
    }

    pub fn logical(&self, id: NodeId) -> u64 {
        self.logical[id as usize]
    }

    pub fn allocated(&self, id: NodeId) -> u64 {
        self.allocated[id as usize]
    }

    pub fn mtime(&self, id: NodeId) -> i64 {
        self.mtime[id as usize]
    }

    pub fn ctime(&self, id: NodeId) -> i64 {
        self.ctime[id as usize]
    }

    pub fn file_id(&self, id: NodeId) -> u64 {
        self.file_id[id as usize]
    }

    pub fn ext_id(&self, id: NodeId) -> u16 {
        self.ext[id as usize]
    }

    pub fn ext_name(&self, id: NodeId) -> &str {
        self.exts.name(self.ext[id as usize])
    }

    pub fn exts(&self) -> &ExtTable {
        &self.exts
    }

    pub fn dir_stats(&self, id: NodeId) -> Option<DirStats> {
        let d = self.dir_ord[id as usize];
        if d == NONE {
            return None;
        }
        let d = d as usize;
        Some(DirStats {
            files: self.dir_files[d],
            dirs: self.dir_dirs[d],
            pending: self.dir_pending[d],
            errors: self.dir_errors[d],
        })
    }

    /// True if the subtree is fully enumerated without errors.
    pub fn is_exact(&self, id: NodeId) -> bool {
        self.dir_stats(id).is_none_or(|s| s.pending == 0 && s.errors == 0)
    }

    pub fn depth(&self, mut id: NodeId) -> usize {
        let mut d = 0;
        while self.parent[id as usize] != NONE {
            id = self.parent[id as usize];
            d += 1;
        }
        d
    }

    /// Full display path of a node.
    pub fn path(&self, id: NodeId) -> String {
        let mut chain = Vec::new();
        let mut cur = id;
        while cur != NONE {
            chain.push(cur);
            cur = self.parent[cur as usize];
        }
        let mut s = String::new();
        for (i, &n) in chain.iter().rev().enumerate() {
            if i > 0 && !s.ends_with('\\') {
                s.push('\\');
            }
            s.push_str(&self.name(n));
        }
        s
    }

    /// Full path as UTF-16 (exact, including unpaired surrogates).
    pub fn path_utf16(&self, id: NodeId) -> Vec<u16> {
        let mut chain = Vec::new();
        let mut cur = id;
        while cur != NONE {
            chain.push(cur);
            cur = self.parent[cur as usize];
        }
        let mut out = Vec::new();
        for (i, &n) in chain.iter().rev().enumerate() {
            if i > 0 && out.last() != Some(&(b'\\' as u16)) {
                out.push(b'\\' as u16);
            }
            self.name_utf16(n, &mut out);
        }
        out
    }

    /// Finds a direct child by exact (case-insensitive ASCII-folded) name.
    pub fn find_child(&self, parent: NodeId, name: &str) -> Option<NodeId> {
        self.children(parent).find(|&c| self.name(c).eq_ignore_ascii_case(name))
    }

    /// Resolves a path relative to the root, using `\` or `/` separators.
    pub fn resolve(&self, rel: &str) -> Option<NodeId> {
        let mut cur = Self::ROOT;
        for part in rel.split(['\\', '/']).filter(|p| !p.is_empty()) {
            cur = self.find_child(cur, part)?;
        }
        Some(cur)
    }

    /// Visits the subtree rooted at `id` in pre-order (excluding deleted nodes).
    pub fn walk(&self, id: NodeId, mut f: impl FnMut(NodeId, usize)) {
        let mut stack = vec![(id, 0usize)];
        while let Some((n, depth)) = stack.pop() {
            f(n, depth);
            for c in self.children(n) {
                stack.push((c, depth + 1));
            }
        }
    }

    /// Approximate heap memory used by the store, in bytes.
    pub fn memory_bytes(&self) -> usize {
        let n = self.parent.capacity();
        n * (4 * 3 + 4 + 2 + 2 + 2 + 4 + 8 * 2 + 8 * 2 + 8 + 4)
            + self.dir_files.capacity() * (8 + 8 + 4 + 4)
            + self.names.capacity()
    }

    /// Releases excess capacity (call after a scan completes).
    pub fn shrink_to_fit(&mut self) {
        macro_rules! s { ($($f:ident),*) => { $( self.$f.shrink_to_fit(); )* } }
        s!(
            parent,
            first_child,
            next_sibling,
            name_off,
            name_len,
            flags,
            ext,
            attrs,
            logical,
            allocated,
            mtime,
            ctime,
            file_id,
            dir_ord,
            dir_files,
            dir_dirs,
            dir_pending,
            dir_errors,
            names
        );
    }

    /// Recomputes every aggregate from leaf data and compares with the stored
    /// values. Used by tests and the `verify` command to detect bookkeeping bugs.
    pub fn check_invariants(&self) -> Result<(), String> {
        let n = self.len();
        let mut logical = vec![0u64; n];
        let mut allocated = vec![0u64; n];
        let mut files = vec![0u64; n];
        let mut dirs = vec![0u64; n];
        let mut pending = vec![0u32; n];
        let mut errors = vec![0u32; n];
        // Post-order accumulate.
        let mut order = Vec::with_capacity(n);
        self.walk(Self::ROOT, |id, _| order.push(id));
        for &id in order.iter().rev() {
            let i = id as usize;
            if self.is_dir(id) {
                if self.flags[i] & flags::PENDING != 0 {
                    pending[i] += 1;
                }
                if self.flags[i] & flags::ERROR != 0 {
                    errors[i] += 1;
                }
            } else {
                logical[i] = self.logical[i];
                allocated[i] = self.allocated[i];
            }
            let p = self.parent[i];
            if p != NONE && id != Self::ROOT {
                let p = p as usize;
                logical[p] += logical[i];
                allocated[p] += allocated[i];
                pending[p] += pending[i];
                errors[p] += errors[i];
                if self.is_dir(id) {
                    dirs[p] += dirs[i] + 1;
                    files[p] += files[i];
                } else {
                    files[p] += 1;
                }
            }
        }
        for &id in &order {
            let i = id as usize;
            if self.logical[i] != logical[i] || self.allocated[i] != allocated[i] {
                return Err(format!("size mismatch at {}: {} vs {}", self.path(id), self.logical[i], logical[i]));
            }
            if let Some(s) = self.dir_stats(id)
                && (s.files, s.dirs, s.pending, s.errors) != (files[i], dirs[i], pending[i], errors[i])
            {
                return Err(format!("dir stats mismatch at {}", self.path(id)));
            }
        }
        Ok(())
    }
}

pub struct Children<'a> {
    tree: &'a Tree,
    next: NodeId,
}

impl Iterator for Children<'_> {
    type Item = NodeId;
    fn next(&mut self) -> Option<NodeId> {
        if self.next == NONE {
            return None;
        }
        let cur = self.next;
        self.next = self.tree.next_sibling[cur as usize];
        Some(cur)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn w(s: &str) -> Vec<u16> {
        s.encode_utf16().collect()
    }

    pub fn file(name: &[u16], size: u64) -> NewNode<'_> {
        NewNode { name, logical: size, allocated: size.div_ceil(4096) * 4096, ..Default::default() }
    }

    pub fn dir(name: &[u16]) -> NewNode<'_> {
        NewNode { name, attrs: attr::DIRECTORY, flags: flags::DIR | flags::PENDING, ..Default::default() }
    }

    /// root/{a.txt(100), sub/{b.MP4(5000), c(0)}, empty/}
    pub fn sample() -> Tree {
        let mut t = Tree::new("C:\\");
        let (a, sub, empty, b, c) = (w("a.txt"), w("sub"), w("empty"), w("b.MP4"), w("c"));
        let dirs = t.add_children(Tree::ROOT, &[file(&a, 100), dir(&sub), dir(&empty)]).unwrap();
        t.finish_dir(Tree::ROOT, false);
        let sub_id = dirs.iter().copied().find(|&d| t.name(d) == "sub").unwrap();
        let empty_id = dirs.iter().copied().find(|&d| t.name(d) == "empty").unwrap();
        t.add_children(sub_id, &[file(&b, 5000), file(&c, 0)]).unwrap();
        t.finish_dir(sub_id, false);
        t.finish_dir(empty_id, false);
        t
    }

    #[test]
    fn aggregates() {
        let t = sample();
        assert_eq!(t.logical(Tree::ROOT), 5100);
        assert_eq!(t.allocated(Tree::ROOT), 4096 + 8192);
        let s = t.dir_stats(Tree::ROOT).unwrap();
        assert_eq!((s.files, s.dirs, s.pending, s.errors), (3, 2, 0, 0));
        t.check_invariants().unwrap();
        assert!(t.is_exact(Tree::ROOT));
    }

    #[test]
    fn paths_and_ext() {
        let t = sample();
        let b = t.resolve("sub\\b.mp4").unwrap();
        assert_eq!(t.path(b), "C:\\sub\\b.MP4");
        assert_eq!(t.ext_name(b), "mp4");
        assert_eq!(t.depth(b), 2);
        assert_eq!(String::from_utf16(&t.path_utf16(b)).unwrap(), "C:\\sub\\b.MP4");
        assert_eq!(extension_of(&w(".gitignore")), None);
        assert_eq!(extension_of(&w("archive.tar.GZ")).as_deref(), Some("gz"));
        assert_eq!(extension_of(&w("trailing.")), None);
    }

    #[test]
    fn pending_and_errors() {
        let mut t = Tree::new("X:\\");
        let d = w("d");
        let ids = t.add_children(Tree::ROOT, &[dir(&d)]).unwrap();
        t.finish_dir(Tree::ROOT, false);
        assert_eq!(t.dir_stats(Tree::ROOT).unwrap().pending, 1);
        assert!(!t.is_exact(Tree::ROOT));
        t.finish_dir(ids[0], true);
        let s = t.dir_stats(Tree::ROOT).unwrap();
        assert_eq!((s.pending, s.errors), (0, 1));
        assert!(t.flags(ids[0]) & flags::ERROR != 0);
        t.check_invariants().unwrap();
    }

    #[test]
    fn remove_updates_ancestors() {
        let mut t = sample();
        let sub = t.resolve("sub").unwrap();
        t.remove(sub);
        assert_eq!(t.logical(Tree::ROOT), 100);
        let s = t.dir_stats(Tree::ROOT).unwrap();
        assert_eq!((s.files, s.dirs), (1, 1));
        assert!(t.resolve("sub").is_none());
        t.check_invariants().unwrap();
        let a = t.resolve("a.txt").unwrap();
        t.remove(a);
        assert_eq!(t.logical(Tree::ROOT), 0);
        t.check_invariants().unwrap();
    }

    #[test]
    fn reparse_helpers() {
        assert!(reparse::is_name_surrogate(reparse::MOUNT_POINT));
        assert!(reparse::is_name_surrogate(reparse::SYMLINK));
        assert!(!reparse::is_name_surrogate(reparse::DEDUP));
        assert!(reparse::is_cloud(0x9000_101A));
        assert!(reparse::is_cloud(reparse::CLOUD));
        assert!(!reparse::is_cloud(reparse::WOF));
    }
}
