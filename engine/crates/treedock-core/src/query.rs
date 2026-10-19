// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Read-only queries over a [`Tree`]: sorting, top files, extension statistics.

use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap};

use serde::Serialize;

use crate::tree::{NodeId, Tree, flags};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize)]
pub enum SortKey {
    Name,
    Logical,
    #[default]
    Allocated,
    Files,
    Modified,
    Extension,
}

impl SortKey {
    pub fn parse(s: &str) -> Option<SortKey> {
        Some(match s.to_ascii_lowercase().as_str() {
            "name" => SortKey::Name,
            "size" | "logical" => SortKey::Logical,
            "allocated" | "disk" => SortKey::Allocated,
            "files" | "count" => SortKey::Files,
            "modified" | "mtime" | "date" => SortKey::Modified,
            "ext" | "extension" | "type" => SortKey::Extension,
            _ => return None,
        })
    }
}

/// Which size measure a query ranks by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize)]
pub enum SizeMeasure {
    Logical,
    #[default]
    Allocated,
}

impl SizeMeasure {
    pub fn of(self, t: &Tree, id: NodeId) -> u64 {
        match self {
            SizeMeasure::Logical => t.logical(id),
            SizeMeasure::Allocated => t.allocated(id),
        }
    }
}

pub fn compare(t: &Tree, key: SortKey, a: NodeId, b: NodeId) -> Ordering {
    let files = |n: NodeId| t.dir_stats(n).map(|s| s.files).unwrap_or(1);
    match key {
        SortKey::Name => natural_cmp(&t.name(a), &t.name(b)),
        SortKey::Logical => t.logical(a).cmp(&t.logical(b)),
        SortKey::Allocated => t.allocated(a).cmp(&t.allocated(b)),
        SortKey::Files => files(a).cmp(&files(b)),
        SortKey::Modified => t.mtime(a).cmp(&t.mtime(b)),
        SortKey::Extension => t.ext_name(a).cmp(t.ext_name(b)),
    }
}

/// Case-insensitive comparison that orders embedded numbers numerically ("file2" < "file10").
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let mut na = String::new();
                while let Some(c) = ai.peek().copied().filter(char::is_ascii_digit) {
                    na.push(c);
                    ai.next();
                }
                let mut nb = String::new();
                while let Some(c) = bi.peek().copied().filter(char::is_ascii_digit) {
                    nb.push(c);
                    bi.next();
                }
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let o = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.to_lowercase().cmp(y.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                ai.next();
                bi.next();
            }
        }
    }
}

/// Children of `id` sorted by `key`. Directories and files are interleaved
/// for size keys (largest first when `descending`); for name sorting,
/// directories come first.
pub fn sorted_children(t: &Tree, id: NodeId, key: SortKey, descending: bool) -> Vec<NodeId> {
    let mut v: Vec<NodeId> = t.children(id).collect();
    v.sort_by(|&a, &b| {
        let dir_first = if key == SortKey::Name { t.is_dir(b).cmp(&t.is_dir(a)) } else { Ordering::Equal };
        let o = compare(t, key, a, b);
        dir_first.then(if descending { o.reverse() } else { o }).then_with(|| a.cmp(&b))
    });
    v
}

/// The `n` largest files under `root`, largest first.
pub fn top_files(t: &Tree, root: NodeId, n: usize, measure: SizeMeasure) -> Vec<NodeId> {
    if n == 0 {
        return Vec::new();
    }
    let mut heap: BinaryHeap<Reverse<(u64, NodeId)>> = BinaryHeap::with_capacity(n + 1);
    t.walk(root, |id, _| {
        if t.is_dir(id) {
            return;
        }
        let s = measure.of(t, id);
        if heap.len() < n {
            heap.push(Reverse((s, id)));
        } else if let Some(Reverse((min, _))) = heap.peek()
            && s > *min
        {
            heap.pop();
            heap.push(Reverse((s, id)));
        }
    });
    let mut v: Vec<(u64, NodeId)> = heap.into_iter().map(|Reverse(x)| x).collect();
    v.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    v.into_iter().map(|(_, id)| id).collect()
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ExtStat {
    pub ext: String,
    pub files: u64,
    pub logical: u64,
    pub allocated: u64,
}

/// Per-extension totals for files under `root`, sorted by allocated size (desc).
pub fn ext_stats(t: &Tree, root: NodeId) -> Vec<ExtStat> {
    let mut m: HashMap<u16, ExtStat> = HashMap::new();
    t.walk(root, |id, _| {
        if t.is_dir(id) {
            return;
        }
        let e =
            m.entry(t.ext_id(id)).or_insert_with(|| ExtStat { ext: t.ext_name(id).to_owned(), ..Default::default() });
        e.files += 1;
        e.logical += t.logical(id);
        e.allocated += t.allocated(id);
    });
    let mut v: Vec<ExtStat> = m.into_values().collect();
    v.sort_by(|a, b| b.allocated.cmp(&a.allocated).then(b.logical.cmp(&a.logical)).then(a.ext.cmp(&b.ext)));
    v
}

/// Directories whose subtree contains no files (excluding links, errors and pending dirs).
pub fn empty_dirs(t: &Tree, root: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    t.walk(root, |id, _| {
        if let Some(s) = t.dir_stats(id) {
            let f = t.flags(id);
            if s.files == 0 && s.pending == 0 && s.errors == 0 && f & flags::SURROGATE == 0 && id != Tree::ROOT {
                let p = t.parent(id);
                // Report only the top-most empty directory of an empty chain.
                if t.dir_stats(p).is_none_or(|ps| ps.files > 0) || p == Tree::ROOT {
                    out.push(id);
                }
            }
        }
    });
    out
}

/// Summary used by reports and the CLI.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Summary {
    pub root: String,
    pub logical: u64,
    pub allocated: u64,
    pub files: u64,
    pub dirs: u64,
    pub errors: u32,
    pub pending: u32,
    pub cloud_files: u64,
    pub cloud_logical: u64,
    pub links_not_followed: u64,
    pub estimated_files: u64,
}

pub fn summary(t: &Tree, root: NodeId) -> Summary {
    let s = t.dir_stats(root).unwrap_or_default();
    let mut sum = Summary {
        root: t.path(root),
        logical: t.logical(root),
        allocated: t.allocated(root),
        files: s.files,
        dirs: s.dirs,
        errors: s.errors,
        pending: s.pending,
        ..Default::default()
    };
    t.walk(root, |id, _| {
        let f = t.flags(id);
        if f & flags::CLOUD != 0 && f & flags::DIR == 0 {
            sum.cloud_files += 1;
            sum.cloud_logical += t.logical(id);
        }
        if f & flags::SURROGATE != 0 {
            sum.links_not_followed += 1;
        }
        if f & flags::ESTIMATED != 0 {
            sum.estimated_files += 1;
        }
    });
    sum
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::tests::sample;

    #[test]
    fn natural_ordering() {
        let mut v = vec!["file10", "File2", "file1", "a", "file02x"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["a", "file1", "File2", "file02x", "file10"]);
    }

    #[test]
    fn sorting_and_top() {
        let t = sample();
        let by_size = sorted_children(&t, Tree::ROOT, SortKey::Logical, true);
        assert_eq!(t.name(by_size[0]), "sub");
        let by_name = sorted_children(&t, Tree::ROOT, SortKey::Name, false);
        let names: Vec<String> = by_name.iter().map(|&n| t.name(n)).collect();
        assert_eq!(names, vec!["empty", "sub", "a.txt"]);
        let top = top_files(&t, Tree::ROOT, 2, SizeMeasure::Logical);
        assert_eq!(top.iter().map(|&n| t.name(n)).collect::<Vec<_>>(), vec!["b.MP4", "a.txt"]);
        assert_eq!(top_files(&t, Tree::ROOT, 10, SizeMeasure::Logical).len(), 3);
    }

    #[test]
    fn ext_and_empty() {
        let t = sample();
        let s = ext_stats(&t, Tree::ROOT);
        assert_eq!(s[0].ext, "mp4");
        assert_eq!(s.iter().map(|e| e.files).sum::<u64>(), 3);
        let e = empty_dirs(&t, Tree::ROOT);
        assert_eq!(e.iter().map(|&n| t.name(n)).collect::<Vec<_>>(), vec!["empty"]);
        let sm = summary(&t, Tree::ROOT);
        assert_eq!((sm.files, sm.dirs, sm.logical), (3, 2, 5100));
    }
}
