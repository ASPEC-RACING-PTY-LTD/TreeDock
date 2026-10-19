// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Squarified treemap layout (Bruls, Huizing & van Wijk, 2000).
//!
//! Layout runs in the engine; the UI only draws the returned rectangles.
//! Items smaller than `min_px` in both dimensions are merged into a single
//! "other" rectangle per directory so output size stays bounded.

use crate::query::SizeMeasure;
use crate::tree::{NodeId, Tree, flags};

pub mod rect_flags {
    pub const DIR: u8 = 1;
    /// Aggregate of items too small to draw individually (node = their parent).
    pub const OTHER: u8 = 2;
    /// Directory whose children were laid out inside this rectangle.
    pub const HAS_CHILDREN: u8 = 4;
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct TreemapRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub node: u32,
    pub depth: u16,
    pub flags: u8,
    pub _pad: u8,
    /// Extension id of a file (0 for directories and aggregates).
    pub ext: u16,
    pub _pad2: u16,
}

#[derive(Clone, Debug)]
pub struct TreemapOptions {
    pub measure: SizeMeasure,
    /// Maximum nesting depth below the root (0 = only the root's children).
    pub max_depth: u16,
    /// Minimum rectangle side in pixels before items are merged.
    pub min_px: f32,
    /// Inset applied to a directory before laying out its children.
    pub padding: f32,
    /// Space reserved at the top of each directory for a label (0 = none).
    pub header: f32,
    /// Hard cap on emitted rectangles.
    pub max_rects: usize,
}

impl Default for TreemapOptions {
    fn default() -> Self {
        TreemapOptions {
            measure: SizeMeasure::Allocated,
            max_depth: 12,
            min_px: 3.0,
            padding: 2.0,
            header: 0.0,
            max_rects: 250_000,
        }
    }
}

#[derive(Clone, Copy)]
struct Area {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

fn worst(row: &[f64], side: f64) -> f64 {
    let sum: f64 = row.iter().sum();
    if sum <= 0.0 || side <= 0.0 {
        return f64::INFINITY;
    }
    let (mut max, mut min) = (f64::MIN, f64::MAX);
    for &r in row {
        max = max.max(r);
        min = min.min(r);
    }
    let s2 = side * side;
    let sum2 = sum * sum;
    (s2 * max / sum2).max(sum2 / (s2 * min))
}

/// Squarifies `areas` (already scaled to pixel areas, sorted descending) into `rect`.
fn squarify(areas: &[f64], rect: Area, out: &mut Vec<Area>) {
    let mut r = rect;
    let mut i = 0;
    while i < areas.len() {
        let side = (r.w.min(r.h)) as f64;
        let mut j = i + 1;
        while j < areas.len() && worst(&areas[i..j + 1], side) <= worst(&areas[i..j], side) {
            j += 1;
        }
        let row = &areas[i..j];
        let sum: f64 = row.iter().sum();
        if r.w >= r.h {
            // Lay the row out as a column on the left.
            let cw = if r.h > 0.0 { (sum / r.h as f64) as f32 } else { 0.0 };
            let mut y = r.y;
            for &a in row {
                let h = if cw > 0.0 { (a / cw as f64) as f32 } else { 0.0 };
                out.push(Area { x: r.x, y, w: cw, h });
                y += h;
            }
            r.x += cw;
            r.w = (r.w - cw).max(0.0);
        } else {
            let rh = if r.w > 0.0 { (sum / r.w as f64) as f32 } else { 0.0 };
            let mut x = r.x;
            for &a in row {
                let w = if rh > 0.0 { (a / rh as f64) as f32 } else { 0.0 };
                out.push(Area { x, y: r.y, w, h: rh });
                x += w;
            }
            r.y += rh;
            r.h = (r.h - rh).max(0.0);
        }
        i = j;
    }
}

/// Lays out the subtree of `root` into a `width` × `height` pixel area.
pub fn layout(tree: &Tree, root: NodeId, width: f32, height: f32, opts: &TreemapOptions) -> Vec<TreemapRect> {
    let mut out = Vec::new();
    if width < 1.0 || height < 1.0 || !tree.is_dir(root) {
        return out;
    }
    let mut stack = vec![(root, Area { x: 0.0, y: 0.0, w: width, h: height }, 0u16)];
    let min_area = (opts.min_px * opts.min_px) as f64;
    let mut areas = Vec::new();
    let mut placed = Vec::new();
    while let Some((node, area, depth)) = stack.pop() {
        if out.len() >= opts.max_rects {
            break;
        }
        let total = opts.measure.of(tree, node) as f64;
        if total <= 0.0 {
            continue;
        }
        let mut kids: Vec<(NodeId, u64)> = tree
            .children(node)
            .filter(|&c| tree.flags(c) & flags::DELETED == 0)
            .map(|c| (c, opts.measure.of(tree, c)))
            .filter(|&(_, s)| s > 0)
            .collect();
        kids.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let scale = (area.w as f64 * area.h as f64) / total;
        // Merge the tail of tiny items into one "other" block.
        let mut cut = kids.len();
        while cut > 1 && (kids[cut - 1].1 as f64 * scale) < min_area {
            cut -= 1;
        }
        let other: u64 = kids[cut..].iter().map(|k| k.1).sum();
        areas.clear();
        areas.extend(kids[..cut].iter().map(|k| k.1 as f64 * scale));
        if other > 0 {
            areas.push(other as f64 * scale);
        }
        // Children of a directory may not fill it exactly (e.g. pending dirs): scale to the children's sum.
        let sum: f64 = areas.iter().sum();
        if sum <= 0.0 {
            continue;
        }
        let fill = (area.w as f64 * area.h as f64) / sum;
        for a in areas.iter_mut() {
            *a *= fill;
        }
        placed.clear();
        squarify(&areas, area, &mut placed);
        for (k, p) in placed.iter().enumerate() {
            let (child, is_other) = if k < cut { (kids[k].0, false) } else { (node, true) };
            let is_dir = !is_other && tree.is_dir(child);
            let mut f = 0u8;
            if is_dir {
                f |= rect_flags::DIR;
            }
            if is_other {
                f |= rect_flags::OTHER;
            }
            // Padding tapers with depth so deep hierarchies keep most of their area.
            let pad = match depth {
                0..=1 => opts.padding,
                2..=4 => opts.padding * 0.5,
                _ => 0.0,
            };
            let header = if depth < 2 { opts.header } else { 0.0 };
            let inner = Area { x: p.x + pad, y: p.y + pad + header, w: p.w - 2.0 * pad, h: p.h - 2.0 * pad - header };
            let descend =
                is_dir && depth < opts.max_depth && inner.w >= opts.min_px * 2.0 && inner.h >= opts.min_px * 2.0;
            if descend && tree.has_children(child) {
                f |= rect_flags::HAS_CHILDREN;
                stack.push((child, inner, depth + 1));
            }
            let ext = if is_dir || is_other { 0 } else { tree.ext_id(child) };
            out.push(TreemapRect {
                x: p.x,
                y: p.y,
                w: p.w,
                h: p.h,
                node: child,
                depth,
                flags: f,
                _pad: 0,
                ext,
                _pad2: 0,
            });
        }
    }
    out
}

/// Returns the deepest rectangle containing the point (for hit-testing).
pub fn hit_test(rects: &[TreemapRect], x: f32, y: f32) -> Option<&TreemapRect> {
    rects
        .iter()
        .filter(|r| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h)
        .max_by_key(|r| (r.depth, r.flags & rect_flags::OTHER == 0))
}

/// One ring segment of a sunburst chart. Angles are fractions of a full turn (0..1).
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct SunburstArc {
    pub node: u32,
    /// Ring index: 0 = the root's children.
    pub depth: u16,
    pub flags: u8,
    pub _pad: u8,
    pub ext: u16,
    pub _pad2: u16,
    pub start: f32,
    pub sweep: f32,
}

/// Sunburst layout of `root`: each ring shows the children of the arc inside it.
/// Arcs narrower than `min_sweep` are merged into one "other" arc per parent.
pub fn sunburst(
    tree: &Tree,
    root: NodeId,
    max_depth: u16,
    min_sweep: f32,
    measure: SizeMeasure,
    cap: usize,
) -> Vec<SunburstArc> {
    let mut out = Vec::new();
    let total = measure.of(tree, root) as f64;
    if total <= 0.0 || !tree.is_dir(root) {
        return out;
    }
    let mut stack = vec![(root, 0f64, 1f64, 0u16)];
    while let Some((node, start, sweep, depth)) = stack.pop() {
        if depth >= max_depth || out.len() >= cap {
            continue;
        }
        let parent_size = measure.of(tree, node) as f64;
        if parent_size <= 0.0 {
            continue;
        }
        let mut kids: Vec<(NodeId, u64)> = tree
            .children(node)
            .filter(|&c| tree.flags(c) & flags::DELETED == 0)
            .map(|c| (c, measure.of(tree, c)))
            .filter(|&(_, s)| s > 0)
            .collect();
        kids.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut a = start;
        let mut other = 0f64;
        for (c, s) in kids {
            let sw = sweep * s as f64 / parent_size;
            if (sw as f32) < min_sweep {
                other += sw;
                continue;
            }
            let is_dir = tree.is_dir(c);
            let ext = if is_dir { 0 } else { tree.ext_id(c) };
            out.push(SunburstArc {
                node: c,
                depth,
                flags: if is_dir { rect_flags::DIR } else { 0 },
                _pad: 0,
                ext,
                _pad2: 0,
                start: a as f32,
                sweep: sw as f32,
            });
            if is_dir && tree.has_children(c) {
                stack.push((c, a, sw, depth + 1));
            }
            a += sw;
        }
        if other > 0.0 {
            out.push(SunburstArc {
                node,
                depth,
                flags: rect_flags::OTHER,
                _pad: 0,
                ext: 0,
                _pad2: 0,
                start: a as f32,
                sweep: other as f32,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::tests::{dir, file, w};

    fn tree() -> Tree {
        let mut t = Tree::new("R:\\");
        let names: Vec<Vec<u16>> = ["a", "b", "c", "d"].iter().map(|s| w(s)).collect();
        let sub = w("sub");
        let mut entries: Vec<_> = names.iter().zip([6u64, 6, 4, 3]).map(|(n, s)| file(n, s * 4096)).collect();
        entries.push(dir(&sub));
        let dirs = t.add_children(Tree::ROOT, &entries).unwrap();
        t.finish_dir(Tree::ROOT, false);
        let inner: Vec<Vec<u16>> = (0..100).map(|i| w(&format!("tiny{i}"))).collect();
        let mut sub_entries: Vec<_> = inner.iter().map(|n| file(n, 1)).collect();
        let big = w("big");
        sub_entries.push(file(&big, 4096 * 5));
        t.add_children(dirs[0], &sub_entries).unwrap();
        t.finish_dir(dirs[0], false);
        t
    }

    #[test]
    fn rectangles_tile_the_area_without_overlap() {
        let t = tree();
        let opts = TreemapOptions { padding: 0.0, max_depth: 0, min_px: 0.0, ..Default::default() };
        let rects = layout(&t, Tree::ROOT, 600.0, 400.0, &opts);
        assert_eq!(rects.len(), 5);
        let area: f32 = rects.iter().map(|r| r.w * r.h).sum();
        assert!((area - 240_000.0).abs() < 1.0, "{area}");
        for (i, a) in rects.iter().enumerate() {
            assert!(a.x >= -0.01 && a.y >= -0.01 && a.x + a.w <= 600.01 && a.y + a.h <= 400.01);
            for b in &rects[i + 1..] {
                let ox = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
                let oy = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
                assert!(ox <= 0.01 || oy <= 0.01, "overlap {a:?} {b:?}");
            }
        }
        // Areas are proportional to sizes: "a" (6 units of 24 total ≈ 25%).
        let a = rects.iter().find(|r| t.name(r.node) == "a").unwrap();
        // Allocated units: a=6, b=6, c=4, d=3, sub=5 + 100 one-cluster files = 105.
        assert!(((a.w * a.h) / 240_000.0 - 6.0 / 124.0).abs() < 0.001);
    }

    #[test]
    fn tiny_items_are_merged_and_nesting_works() {
        let t = tree();
        let opts = TreemapOptions { measure: SizeMeasure::Logical, ..Default::default() };
        let rects = layout(&t, Tree::ROOT, 800.0, 600.0, &opts);
        let sub = t.resolve("sub").unwrap();
        let sub_rect = rects.iter().find(|r| r.node == sub && r.flags & rect_flags::OTHER == 0).unwrap();
        assert!(sub_rect.flags & rect_flags::HAS_CHILDREN != 0);
        let others: Vec<_> = rects.iter().filter(|r| r.flags & rect_flags::OTHER != 0).collect();
        assert_eq!(others.len(), 1);
        assert_eq!(others[0].node, sub);
        // 100 tiny files must not produce 100 rectangles.
        assert!(rects.len() < 12, "{}", rects.len());
        let big = t.resolve("sub\\big").unwrap();
        let br = rects.iter().find(|r| r.node == big).unwrap();
        let hit = hit_test(&rects, br.x + br.w / 2.0, br.y + br.h / 2.0).unwrap();
        assert_eq!(hit.node, big);
    }

    #[test]
    fn sunburst_rings_partition_their_parent() {
        let t = tree();
        let arcs = sunburst(&t, Tree::ROOT, 4, 0.001, SizeMeasure::Allocated, 10_000);
        let ring0: f32 = arcs.iter().filter(|a| a.depth == 0).map(|a| a.sweep).sum();
        assert!((ring0 - 1.0).abs() < 1e-4, "{ring0}");
        let sub = t.resolve("sub").unwrap();
        let s = arcs.iter().find(|a| a.node == sub && a.depth == 0).unwrap();
        let inner: f32 = arcs.iter().filter(|a| a.depth == 1).map(|a| a.sweep).sum();
        assert!((inner - s.sweep).abs() < 1e-4);
        assert!(
            arcs.iter()
                .filter(|a| a.depth == 1)
                .all(|a| a.start >= s.start - 1e-5 && a.start + a.sweep <= s.start + s.sweep + 1e-4)
        );
        // Tiny items merge into one "other" arc.
        let merged = sunburst(&t, Tree::ROOT, 4, 0.05, SizeMeasure::Logical, 10_000);
        assert!(merged.iter().any(|a| a.flags & rect_flags::OTHER != 0));
        assert!(merged.len() < 10);
    }

    #[test]
    fn squarified_aspect_ratios_are_reasonable() {
        let mut t = Tree::new("R:\\");
        let names: Vec<Vec<u16>> = (0..50).map(|i| w(&format!("f{i}"))).collect();
        let entries: Vec<_> = names.iter().enumerate().map(|(i, n)| file(n, (i as u64 + 1) * 40960)).collect();
        t.add_children(Tree::ROOT, &entries).unwrap();
        t.finish_dir(Tree::ROOT, false);
        let rects = layout(&t, Tree::ROOT, 1000.0, 700.0, &TreemapOptions { padding: 0.0, ..Default::default() });
        let worst = rects.iter().map(|r| (r.w / r.h).max(r.h / r.w)).fold(0.0f32, f32::max);
        assert!(worst < 6.0, "worst aspect {worst}");
    }
}
