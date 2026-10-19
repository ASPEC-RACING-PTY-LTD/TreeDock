// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Accumulates parsed MFT records and links them into a [`Tree`].
//!
//! Extension records are folded into their base record by following the
//! base reference stored in each extension record, so `$ATTRIBUTE_LIST`
//! values never need to be read. Hard links (multiple `$FILE_NAME`s) appear
//! at every path; their size is attributed according to [`HardlinkPolicy`].

use treedock_core::tree::{NewNode, NodeId, Tree, attr, flags, reparse};

use crate::record::{self, NS_DOS, ParsedRecord, RecordError};

pub const ROOT_RECORD: u64 = 5;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HardlinkPolicy {
    /// Full size at the first path reached; other links show size 0 (default).
    #[default]
    Once,
    /// Size divided equally across links.
    Divide,
    /// Full size at every path (Explorer-like; totals can exceed used space).
    Every,
}

impl HardlinkPolicy {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_ascii_lowercase().as_str() {
            "once" => HardlinkPolicy::Once,
            "divide" | "split" => HardlinkPolicy::Divide,
            "every" | "all" => HardlinkPolicy::Every,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug)]
pub struct BuildOptions {
    pub hardlinks: HardlinkPolicy,
    /// Count alternate data streams in file sizes.
    pub include_ads: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        BuildOptions { hardlinks: HardlinkPolicy::Once, include_ads: true }
    }
}

const F_IN_USE: u8 = 1;
const F_DIR: u8 = 2;
const F_BASE_SEEN: u8 = 4;

struct NameEntry {
    record: u32,
    parent: u64,
    off: u32,
    len: u8,
}

#[derive(Default)]
pub struct MftIndex {
    seq: Vec<u16>,
    rflags: Vec<u8>,
    attrs: Vec<u32>,
    ctime: Vec<i64>,
    mtime: Vec<i64>,
    logical: Vec<u64>,
    allocated: Vec<u64>,
    ads_logical: Vec<u64>,
    ads_allocated: Vec<u64>,
    reparse: Vec<u32>,
    names: Vec<NameEntry>,
    pool: Vec<u16>,
    /// Records that failed to parse (corrupt or torn).
    pub bad_records: u64,
    pub records_in_use: u64,
}

/// Statistics from building a tree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildStats {
    pub nodes: u64,
    pub orphans: u64,
    pub hardlinked_files: u64,
    pub extra_links: u64,
}

impl MftIndex {
    pub fn with_capacity(records: usize) -> Self {
        let mut s = MftIndex::default();
        s.ensure(records.saturating_sub(1));
        s
    }

    fn ensure(&mut self, rec: usize) {
        if rec < self.seq.len() {
            return;
        }
        let n = rec + 1;
        self.seq.resize(n, 0);
        self.rflags.resize(n, 0);
        self.attrs.resize(n, 0);
        self.ctime.resize(n, 0);
        self.mtime.resize(n, 0);
        self.logical.resize(n, 0);
        self.allocated.resize(n, 0);
        self.ads_logical.resize(n, 0);
        self.ads_allocated.resize(n, 0);
        self.reparse.resize(n, 0);
    }

    pub fn record_count(&self) -> usize {
        self.seq.len()
    }

    /// Applies fixups to `raw` and ingests it. Unused records are ignored.
    /// Records numbers above 2^32 are rejected (u32 node limit).
    pub fn ingest_raw(&mut self, record_no: u64, raw: &mut [u8], sector: usize) -> Result<(), RecordError> {
        // Unused (zeroed) slots are common: skip quickly without counting as errors.
        if raw.get(0..4) != Some(b"FILE") {
            return Ok(());
        }
        let r = record::apply_fixups(raw, sector).and_then(|_| record::parse_record(raw));
        match r {
            Ok(p) => {
                self.ingest(record_no, &p);
                Ok(())
            }
            Err(e) => {
                self.bad_records += 1;
                Err(e)
            }
        }
    }

    pub fn ingest(&mut self, record_no: u64, p: &ParsedRecord) {
        if !p.in_use() || record_no >= u32::MAX as u64 {
            return;
        }
        let base = record::ref_record(p.base);
        let target = if base != 0 { base } else { record_no };
        if target >= u32::MAX as u64 {
            return;
        }
        let t = target as usize;
        self.ensure(t);
        if base == 0 {
            self.records_in_use += 1;
            self.seq[t] = p.seq;
            self.rflags[t] |= F_IN_USE | F_BASE_SEEN | if p.is_dir() { F_DIR } else { 0 };
            if let Some(a) = p.std_attrs {
                self.attrs[t] = a;
            }
            self.ctime[t] = p.ctime;
            self.mtime[t] = p.mtime;
        }
        if p.reparse_tag != 0 {
            self.reparse[t] = p.reparse_tag;
        }
        for n in &p.names {
            if n.namespace == NS_DOS || n.name.len() > 255 {
                continue;
            }
            self.names.push(NameEntry {
                record: t as u32,
                parent: n.parent,
                off: self.pool.len() as u32,
                len: n.name.len() as u8,
            });
            self.pool.extend_from_slice(&n.name);
        }
        for d in &p.data {
            if d.name.is_empty() {
                self.logical[t] = d.logical;
                self.allocated[t] = d.allocated;
            } else {
                self.ads_logical[t] += d.logical;
                self.ads_allocated[t] += d.allocated;
            }
        }
    }

    fn in_use_dir(&self, rec: u64, seq: u16) -> bool {
        let r = rec as usize;
        r < self.seq.len()
            && self.rflags[r] & (F_IN_USE | F_BASE_SEEN | F_DIR) == (F_IN_USE | F_BASE_SEEN | F_DIR)
            && (seq == 0 || self.seq[r] == seq)
    }

    fn node_flags(&self, r: usize) -> (u16, u32) {
        let mut a = self.attrs[r];
        let mut f = 0u16;
        if self.rflags[r] & F_DIR != 0 {
            a |= attr::DIRECTORY;
            f |= flags::DIR;
        }
        let tag = self.reparse[r];
        if tag != 0 {
            a |= attr::REPARSE_POINT;
            f |= flags::REPARSE;
            if tag != u32::MAX && reparse::is_name_surrogate(tag) {
                f |= flags::SURROGATE;
            }
            if reparse::is_cloud(tag) {
                f |= flags::CLOUD;
            }
        }
        if a & (attr::RECALL_ON_OPEN | attr::RECALL_ON_DATA_ACCESS) != 0 {
            f |= flags::CLOUD;
        }
        if a & attr::COMPRESSED != 0 {
            f |= flags::COMPRESSED;
        }
        if a & attr::SPARSE_FILE != 0 {
            f |= flags::SPARSE;
        }
        if a & attr::OFFLINE != 0 {
            f |= flags::OFFLINE;
        }
        if f & flags::DIR != 0 && f & flags::SURROGATE == 0 {
            f |= flags::PENDING;
        }
        (f, a)
    }

    /// Links the index into a tree rooted at record 5.
    pub fn build_tree(&self, root_display: &str, opts: &BuildOptions) -> (Tree, BuildStats) {
        let mut tree = Tree::new(root_display);
        let mut stats = BuildStats::default();
        let n = self.seq.len();
        if n <= ROOT_RECORD as usize {
            tree.finish_dir(Tree::ROOT, true);
            return (tree, stats);
        }
        // Valid names grouped by parent record (CSR layout).
        let mut link_count = vec![0u16; n];
        let mut by_parent_count = vec![0u32; n + 1];
        let mut orphans: Vec<usize> = Vec::new();
        let mut valid = vec![false; self.names.len()];
        for (i, e) in self.names.iter().enumerate() {
            let r = e.record as usize;
            if r == ROOT_RECORD as usize || self.rflags[r] & F_BASE_SEEN == 0 {
                continue;
            }
            link_count[r] = link_count[r].saturating_add(1);
            let p = record::ref_record(e.parent);
            if p != e.record as u64 && self.in_use_dir(p, record::ref_seq(e.parent)) {
                valid[i] = true;
                by_parent_count[p as usize] += 1;
            } else {
                orphans.push(i);
            }
        }
        let mut start = vec![0u32; n + 1];
        for i in 0..n {
            start[i + 1] = start[i] + by_parent_count[i];
        }
        let mut fill = start.clone();
        let mut children = vec![0u32; start[n] as usize];
        for (i, e) in self.names.iter().enumerate() {
            if valid[i] {
                let p = record::ref_record(e.parent) as usize;
                children[fill[p] as usize] = i as u32;
                fill[p] += 1;
            }
        }
        stats.hardlinked_files = link_count.iter().filter(|&&c| c > 1).count() as u64;

        let mut size_given = vec![false; n];
        let mut dir_visited = vec![false; n];
        dir_visited[ROOT_RECORD as usize] = true;
        let mut queue: std::collections::VecDeque<(usize, NodeId)> = std::collections::VecDeque::new();
        queue.push_back((ROOT_RECORD as usize, Tree::ROOT));
        // Root metadata.
        tree_root_times(&mut tree, self);

        let mut name_buf: Vec<u16> = Vec::new();
        let mut entries: Vec<(usize, usize, usize, NewNode<'static>)> = Vec::new();
        while let Some((rec, node)) = queue.pop_front() {
            let kids = &children[start[rec] as usize..start[rec + 1] as usize];
            self.emit(
                kids.iter().map(|&i| i as usize),
                opts,
                &link_count,
                &mut size_given,
                &mut stats,
                &mut name_buf,
                &mut entries,
            );
            let nodes: Vec<NewNode> =
                entries.iter().map(|(_, s, l, nn)| NewNode { name: &name_buf[*s..*s + *l], ..nn.clone() }).collect();
            let new_dirs = match tree.add_children(node, &nodes) {
                Ok(d) => d,
                Err(_) => {
                    tree.finish_dir(node, true);
                    break;
                }
            };
            tree.finish_dir(node, false);
            let pending_recs =
                entries.iter().filter(|(_, _, _, nn)| nn.flags & flags::PENDING != 0).map(|(r, _, _, _)| *r);
            for (child_rec, child_node) in pending_recs.zip(new_dirs) {
                if dir_visited[child_rec] {
                    // A directory reached twice indicates corruption: do not descend again.
                    tree.finish_dir(child_node, true);
                } else {
                    dir_visited[child_rec] = true;
                    queue.push_back((child_rec, child_node));
                }
            }
            name_buf.clear();
            entries.clear();
        }

        if !orphans.is_empty() {
            stats.orphans = orphans.len() as u64;
            let orphan_name: Vec<u16> = "$Orphaned".encode_utf16().collect();
            let dir = NewNode {
                name: &orphan_name,
                attrs: attr::DIRECTORY,
                flags: flags::DIR | flags::PENDING,
                ..Default::default()
            };
            if let Ok(ids) = tree.add_children(Tree::ROOT, &[dir]) {
                let onode = ids[0];
                // Orphaned directories are listed but not descended (their parent chain is broken).
                self.emit(
                    orphans.iter().copied(),
                    opts,
                    &link_count,
                    &mut size_given,
                    &mut stats,
                    &mut name_buf,
                    &mut entries,
                );
                let nodes: Vec<NewNode> = entries
                    .iter()
                    .map(|(_, s, l, nn)| {
                        let mut nn = NewNode { name: &name_buf[*s..*s + *l], ..nn.clone() };
                        nn.flags &= !flags::PENDING;
                        nn
                    })
                    .collect();
                let _ = tree.add_children(onode, &nodes);
                tree.finish_dir(onode, false);
            }
        }
        stats.nodes = tree.len() as u64;
        (tree, stats)
    }

    #[allow(clippy::too_many_arguments)]
    fn emit(
        &self,
        name_idx: impl Iterator<Item = usize>,
        opts: &BuildOptions,
        link_count: &[u16],
        size_given: &mut [bool],
        stats: &mut BuildStats,
        name_buf: &mut Vec<u16>,
        entries: &mut Vec<(usize, usize, usize, NewNode<'static>)>,
    ) {
        for i in name_idx {
            let e = &self.names[i];
            let r = e.record as usize;
            let (mut f, a) = self.node_flags(r);
            let is_dir = f & flags::DIR != 0;
            let (mut logical, mut allocated) = (self.logical[r], self.allocated[r]);
            if opts.include_ads {
                logical += self.ads_logical[r];
                allocated += self.ads_allocated[r];
            }
            let links = link_count[r].max(1) as u64;
            if !is_dir && links > 1 {
                f |= flags::HARDLINK;
                match opts.hardlinks {
                    HardlinkPolicy::Every => {}
                    HardlinkPolicy::Divide => {
                        logical /= links;
                        allocated /= links;
                    }
                    HardlinkPolicy::Once => {
                        if size_given[r] {
                            logical = 0;
                            allocated = 0;
                            stats.extra_links += 1;
                        } else {
                            size_given[r] = true;
                        }
                    }
                }
            }
            if is_dir {
                logical = 0;
                allocated = 0;
            }
            let s = name_buf.len();
            name_buf.extend_from_slice(&self.pool[e.off as usize..e.off as usize + e.len as usize]);
            entries.push((
                r,
                s,
                e.len as usize,
                NewNode {
                    name: &[],
                    attrs: a,
                    logical,
                    allocated,
                    mtime: self.mtime[r],
                    ctime: self.ctime[r],
                    file_id: ((self.seq[r] as u64) << 48) | r as u64,
                    flags: f,
                },
            ));
        }
    }
}

fn tree_root_times(tree: &mut Tree, idx: &MftIndex) {
    let r = ROOT_RECORD as usize;
    if r < idx.mtime.len() {
        tree.set_root_times(idx.mtime[r], idx.ctime[r]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::builder::RecordBuilder;
    use crate::record::{NS_DOS, NS_POSIX, NS_WIN32, NS_WIN32_AND_DOS, RECORD_IN_USE, RECORD_IS_DIRECTORY};

    const DIR: u16 = RECORD_IN_USE | RECORD_IS_DIRECTORY;
    const FILE: u16 = RECORD_IN_USE;
    fn r(rec: u64, seq: u16) -> u64 {
        ((seq as u64) << 48) | rec
    }
    fn data(alloc: u64, size: u64) -> (u64, u64) {
        (alloc, size)
    }

    fn fixture() -> MftIndex {
        let mut idx = MftIndex::default();
        let mut add = |no: u64, b: Vec<u8>| idx.ingest_raw(no, &mut b.clone(), 512).unwrap();
        let rl = [0x11, 0x01, 0x10, 0x00];
        add(
            0,
            RecordBuilder::new(1, FILE)
                .file_name(r(5, 5), NS_WIN32_AND_DOS, "$MFT")
                .data_nonresident("", 0, 1 << 20, 1 << 20, None, &rl)
                .build(),
        );
        add(5, RecordBuilder::new(5, DIR).std_info(0x16, 10, 20).file_name(r(5, 5), NS_WIN32_AND_DOS, ".").build());
        add(
            30,
            RecordBuilder::new(2, DIR)
                .file_name(r(5, 5), NS_WIN32, "Documents")
                .file_name(r(5, 5), NS_DOS, "DOCUME~1")
                .build(),
        );
        let (a, s) = data(8192, 5000);
        add(
            31,
            RecordBuilder::new(1, FILE)
                .std_info(0x20, 1, 2)
                .file_name(r(30, 2), NS_POSIX, "report.PDF")
                .data_nonresident("", 0, a, s, None, &rl)
                .data_resident("Zone.Identifier", 100)
                .build(),
        );
        // Hard link: two names in different folders.
        add(
            32,
            RecordBuilder::new(1, FILE)
                .file_name(r(5, 5), NS_WIN32, "link-a.dll")
                .file_name(r(30, 2), NS_WIN32, "link-b.dll")
                .data_nonresident("", 0, 40960, 40000, None, &rl)
                .build(),
        );
        // File whose data lives in an extension record (33 → base 34).
        add(
            33,
            RecordBuilder::new(1, FILE)
                .base(r(34, 4))
                .data_nonresident("", 0, 1 << 30, (1 << 30) - 5, None, &rl)
                .build(),
        );
        add(34, RecordBuilder::new(4, FILE).attribute_list().file_name(r(30, 2), NS_WIN32, "big.vhdx").build());
        // Orphan: parent record 99 does not exist.
        add(35, RecordBuilder::new(1, FILE).file_name(r(99, 1), NS_WIN32, "lost.tmp").data_resident("", 10).build());
        // Stale parent reference: sequence mismatch for record 30.
        add(36, RecordBuilder::new(1, FILE).file_name(r(30, 9), NS_WIN32, "stale.txt").data_resident("", 1).build());
        // Junction directory.
        add(37, RecordBuilder::new(1, DIR).reparse(0xA000_0003).file_name(r(5, 5), NS_WIN32, "Junction").build());
        // Deleted (not in use) file must be ignored.
        add(38, RecordBuilder::new(1, 0).file_name(r(5, 5), NS_WIN32, "deleted.txt").data_resident("", 1).build());
        // Compressed file.
        add(
            39,
            RecordBuilder::new(1, FILE)
                .std_info(0x800, 0, 0)
                .file_name(r(30, 2), NS_WIN32, "packed.log")
                .data_nonresident("", 0, 65536, 60000, Some(4096), &rl)
                .build(),
        );
        idx
    }

    #[test]
    fn builds_expected_tree() {
        let idx = fixture();
        let (t, stats) = idx.build_tree("C:\\", &BuildOptions::default());
        t.check_invariants().unwrap();
        assert!(t.is_exact(Tree::ROOT));
        let names = |p: &str| {
            let id = t.resolve(p).unwrap();
            let mut v: Vec<String> = t.children(id).map(|c| t.name(c)).collect();
            v.sort();
            v
        };
        assert_eq!(names(""), vec!["$MFT", "$Orphaned", "Documents", "Junction", "link-a.dll"]);
        assert_eq!(names("Documents"), vec!["big.vhdx", "link-b.dll", "packed.log", "report.PDF"]);
        assert_eq!(names("$Orphaned"), vec!["lost.tmp", "stale.txt"]);
        let rep = t.resolve("Documents\\report.pdf").unwrap();
        assert_eq!(t.logical(rep), 5100); // includes the ADS
        assert_eq!(t.allocated(rep), 8192);
        assert_eq!(t.mtime(rep), 2);
        assert_eq!(t.allocated(t.resolve("Documents\\big.vhdx").unwrap()), 1 << 30);
        let packed = t.resolve("Documents\\packed.log").unwrap();
        assert_eq!(t.allocated(packed), 4096);
        assert!(t.flags(packed) & flags::COMPRESSED != 0);
        let j = t.resolve("Junction").unwrap();
        assert!(t.flags(j) & flags::SURROGATE != 0);
        assert_eq!(stats.orphans, 2);
        assert_eq!(stats.hardlinked_files, 1);
        assert_eq!(stats.extra_links, 1);
        // Once policy: the hard-linked file's 40960 bytes counted exactly once.
        let total = (1 << 20) + 8192 + 40960 + (1 << 30) + 4096;
        assert_eq!(t.allocated(Tree::ROOT), total);
        assert_eq!(t.mtime(Tree::ROOT), 20);
    }

    #[test]
    fn hardlink_policies() {
        let idx = fixture();
        let alloc = |p: HardlinkPolicy| {
            let (t, _) = idx.build_tree("C:\\", &BuildOptions { hardlinks: p, include_ads: false });
            let a = t.allocated(t.resolve("link-a.dll").unwrap());
            let b = t.allocated(t.resolve("Documents\\link-b.dll").unwrap());
            assert!(t.flags(t.resolve("link-a.dll").unwrap()) & flags::HARDLINK != 0);
            (a, b)
        };
        let (a, b) = alloc(HardlinkPolicy::Once);
        assert_eq!(a + b, 40960);
        assert_eq!(alloc(HardlinkPolicy::Divide), (20480, 20480));
        assert_eq!(alloc(HardlinkPolicy::Every), (40960, 40960));
    }

    #[test]
    fn corrupt_records_are_counted_not_fatal() {
        let mut idx = fixture();
        let mut bad = RecordBuilder::new(1, FILE).file_name(r(5, 5), NS_WIN32, "x").build();
        bad[510] ^= 0xFF;
        assert!(idx.ingest_raw(40, &mut bad, 512).is_err());
        assert_eq!(idx.bad_records, 1);
        let mut zero = vec![0u8; 1024];
        assert!(idx.ingest_raw(41, &mut zero, 512).is_ok());
        let (t, _) = idx.build_tree("C:\\", &BuildOptions::default());
        t.check_invariants().unwrap();
    }

    #[test]
    fn directory_cycles_do_not_loop() {
        let mut idx = MftIndex::default();
        let mut add = |no: u64, b: Vec<u8>| idx.ingest_raw(no, &mut b.clone(), 512).unwrap();
        add(5, RecordBuilder::new(5, DIR).file_name(r(5, 5), NS_WIN32, ".").build());
        // Directory 40 has two names (corrupt: directories cannot be hard linked)
        // and one of them makes it its own descendant.
        add(
            40,
            RecordBuilder::new(1, DIR).file_name(r(5, 5), NS_WIN32, "a").file_name(r(41, 1), NS_WIN32, "loop").build(),
        );
        add(41, RecordBuilder::new(1, DIR).file_name(r(40, 1), NS_WIN32, "b").build());
        let (t, _) = idx.build_tree("C:\\", &BuildOptions::default());
        t.check_invariants().unwrap();
        assert!(t.len() < 10);
    }
}
