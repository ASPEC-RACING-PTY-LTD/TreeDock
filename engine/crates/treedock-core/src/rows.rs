// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Flattened, virtualisable tree-list model (MASTER_PLAN §3.5, §6.6).
//!
//! The UI renders a flat list of visible rows (node + depth). Expanding a row
//! splices its sorted children in after it; collapsing removes its
//! descendants. The cost of every operation is proportional to the rows that
//! change, never to the size of the whole tree.

use std::collections::HashSet;

use crate::query::{SortKey, sorted_children};
use crate::tree::{NodeId, Tree, flags};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub node: NodeId,
    pub depth: u16,
}

#[derive(Clone, Debug)]
pub struct FlatView {
    root: NodeId,
    rows: Vec<Row>,
    expanded: HashSet<NodeId>,
    sort: SortKey,
    descending: bool,
}

impl FlatView {
    /// Creates a view showing `root` expanded.
    pub fn new(tree: &Tree, root: NodeId, sort: SortKey, descending: bool) -> Self {
        let mut v = FlatView { root, rows: Vec::new(), expanded: HashSet::new(), sort, descending };
        v.expanded.insert(root);
        v.rebuild(tree);
        v
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn row(&self, i: usize) -> Option<Row> {
        self.rows.get(i).copied()
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn sort(&self) -> (SortKey, bool) {
        (self.sort, self.descending)
    }

    pub fn is_expanded(&self, node: NodeId) -> bool {
        self.expanded.contains(&node)
    }

    fn visible_children(tree: &Tree, node: NodeId, key: SortKey, desc: bool) -> Vec<NodeId> {
        let mut v = sorted_children(tree, node, key, desc);
        v.retain(|&c| tree.flags(c) & flags::DELETED == 0);
        v
    }

    /// Appends `node`'s expanded subtree rows (excluding `node` itself) to `out`.
    fn emit_subtree(&self, tree: &Tree, node: NodeId, depth: u16, out: &mut Vec<Row>) {
        // Iterative DFS preserving sorted order.
        let mut stack: Vec<(NodeId, u16)> = Vec::new();
        for c in Self::visible_children(tree, node, self.sort, self.descending).into_iter().rev() {
            stack.push((c, depth + 1));
        }
        while let Some((n, d)) = stack.pop() {
            out.push(Row { node: n, depth: d });
            if self.expanded.contains(&n) && tree.is_dir(n) {
                for c in Self::visible_children(tree, n, self.sort, self.descending).into_iter().rev() {
                    stack.push((c, d + 1));
                }
            }
        }
    }

    /// Recomputes all rows from the expansion state (after a sort change,
    /// scan progress or file operations). Cost is proportional to visible rows.
    pub fn rebuild(&mut self, tree: &Tree) {
        let mut rows = vec![Row { node: self.root, depth: 0 }];
        if self.expanded.contains(&self.root) {
            self.emit_subtree(tree, self.root, 0, &mut rows);
        }
        // Forget expansion of nodes that disappeared.
        self.rows = rows;
    }

    pub fn set_sort(&mut self, tree: &Tree, key: SortKey, descending: bool) {
        self.sort = key;
        self.descending = descending;
        self.rebuild(tree);
    }

    /// Expands row `i`. Returns the number of rows inserted.
    pub fn expand(&mut self, tree: &Tree, i: usize) -> usize {
        let Some(r) = self.row(i) else { return 0 };
        if !tree.is_dir(r.node) || !self.expanded.insert(r.node) {
            return 0;
        }
        let mut sub = Vec::new();
        self.emit_subtree(tree, r.node, r.depth, &mut sub);
        let n = sub.len();
        self.rows.splice(i + 1..i + 1, sub);
        n
    }

    /// Collapses row `i`. Returns the number of rows removed.
    pub fn collapse(&mut self, i: usize) -> usize {
        let Some(r) = self.row(i) else { return 0 };
        if !self.expanded.remove(&r.node) {
            return 0;
        }
        let end =
            self.rows[i + 1..].iter().position(|x| x.depth <= r.depth).map(|p| i + 1 + p).unwrap_or(self.rows.len());
        self.rows.drain(i + 1..end);
        end - i - 1
    }

    pub fn toggle(&mut self, tree: &Tree, i: usize) {
        match self.row(i) {
            Some(r) if self.expanded.contains(&r.node) => {
                self.collapse(i);
            }
            Some(_) => {
                self.expand(tree, i);
            }
            None => {}
        }
    }

    pub fn index_of(&self, node: NodeId) -> Option<usize> {
        self.rows.iter().position(|r| r.node == node)
    }

    /// Expands all ancestors of `node` so it becomes visible; returns its row.
    pub fn reveal(&mut self, tree: &Tree, node: NodeId) -> Option<usize> {
        let mut chain = Vec::new();
        let mut cur = tree.parent(node);
        while cur != crate::tree::NONE {
            chain.push(cur);
            if cur == self.root {
                break;
            }
            cur = tree.parent(cur);
        }
        if node != self.root && chain.last() != Some(&self.root) {
            return None; // not under this view's root
        }
        let mut changed = false;
        for &a in chain.iter().rev() {
            changed |= self.expanded.insert(a);
        }
        if changed {
            self.rebuild(tree);
        }
        self.index_of(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::tests::sample;

    fn names(t: &Tree, v: &FlatView) -> Vec<String> {
        v.rows().iter().map(|r| format!("{}{}", "  ".repeat(r.depth as usize), t.name(r.node))).collect()
    }

    #[test]
    fn expand_collapse_reveal() {
        let t = sample();
        let mut v = FlatView::new(&t, Tree::ROOT, SortKey::Allocated, true);
        assert_eq!(names(&t, &v), vec!["C:\\", "  sub", "  a.txt", "  empty"]);
        assert_eq!(v.expand(&t, 1), 2);
        assert_eq!(names(&t, &v), vec!["C:\\", "  sub", "    b.MP4", "    c", "  a.txt", "  empty"]);
        assert_eq!(v.expand(&t, 1), 0); // already expanded
        assert_eq!(v.collapse(1), 2);
        assert_eq!(v.len(), 4);
        let c = t.resolve("sub\\c").unwrap();
        let i = v.reveal(&t, c).unwrap();
        assert_eq!(v.row(i).unwrap().node, c);
        v.set_sort(&t, SortKey::Name, false);
        assert_eq!(names(&t, &v), vec!["C:\\", "  empty", "  sub", "    b.MP4", "    c", "  a.txt"]);
        v.collapse(0);
        assert_eq!(v.len(), 1);
        v.toggle(&t, 0);
        assert_eq!(v.len(), 6);
    }

    #[test]
    fn rebuild_after_removal() {
        let mut t = sample();
        let mut v = FlatView::new(&t, Tree::ROOT, SortKey::Allocated, true);
        v.expand(&t, 1);
        let sub = t.resolve("sub").unwrap();
        t.remove(sub);
        v.rebuild(&t);
        assert_eq!(names(&t, &v), vec!["C:\\", "  a.txt", "  empty"]);
    }

    #[test]
    fn large_directory_expand_is_fast() {
        let mut t = Tree::new("X:\\");
        let names: Vec<Vec<u16>> = (0..200_000).map(|i| format!("f{i}").encode_utf16().collect()).collect();
        let nodes: Vec<crate::tree::NewNode> =
            names.iter().enumerate().map(|(i, n)| crate::tree::tests::file(n, i as u64)).collect();
        t.add_children(Tree::ROOT, &nodes).unwrap();
        t.finish_dir(Tree::ROOT, false);
        let start = std::time::Instant::now();
        let mut v = FlatView::new(&t, Tree::ROOT, SortKey::Allocated, true);
        assert_eq!(v.len(), 200_001);
        v.collapse(0);
        v.expand(&t, 0);
        assert!(start.elapsed() < std::time::Duration::from_secs(2), "{:?}", start.elapsed());
    }
}
