// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! CSV and JSON exporters (MASTER_PLAN §4.10). Both stream, so memory stays flat.

use std::io::{self, Write};

use crate::query::{self, SortKey};
use crate::time::filetime_to_iso;
use crate::tree::{NodeId, Tree, flags};

pub const JSON_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub struct ExportOptions {
    /// Maximum depth below the export root (0 = root only). `None` = unlimited.
    pub max_depth: Option<usize>,
    /// Include files (otherwise directories only).
    pub include_files: bool,
    /// CSV: neutralise cells that spreadsheets would treat as formulas.
    pub spreadsheet_safe: bool,
    /// CSV: write a UTF-8 byte-order mark (helps Excel detect UTF-8).
    pub bom: bool,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions { max_depth: None, include_files: true, spreadsheet_safe: true, bom: false }
    }
}

pub fn flag_names(f: u16) -> Vec<&'static str> {
    const NAMES: [(u16, &str); 11] = [
        (flags::REPARSE, "reparse"),
        (flags::SURROGATE, "link_not_followed"),
        (flags::HARDLINK, "hardlink"),
        (flags::COMPRESSED, "compressed"),
        (flags::SPARSE, "sparse"),
        (flags::CLOUD, "cloud"),
        (flags::ERROR, "error"),
        (flags::PENDING, "pending"),
        (flags::ESTIMATED, "estimated"),
        (flags::DELETED, "deleted"),
        (flags::OFFLINE, "offline"),
    ];
    NAMES.iter().filter(|(b, _)| f & b != 0).map(|(_, n)| *n).collect()
}

fn csv_field(out: &mut impl Write, s: &str, safe: bool) -> io::Result<()> {
    let formula = safe && s.starts_with(['=', '+', '-', '@', '\t', '\r']);
    let needs_quotes = formula || s.contains([',', '"', '\n', '\r']) || s.starts_with(' ') || s.ends_with(' ');
    if !needs_quotes {
        return out.write_all(s.as_bytes());
    }
    out.write_all(b"\"")?;
    if formula {
        out.write_all(b"'")?;
    }
    out.write_all(s.replace('"', "\"\"").as_bytes())?;
    out.write_all(b"\"")
}

/// Writes one CSV row per node (pre-order, children sorted by allocated size).
pub fn write_csv(t: &Tree, root: NodeId, opts: &ExportOptions, out: &mut impl Write) -> io::Result<()> {
    if opts.bom {
        out.write_all(b"\xEF\xBB\xBF")?;
    }
    out.write_all(b"Path,Name,Type,Size,Allocated,Files,Folders,Modified,Created,Extension,Attributes,Flags\r\n")?;
    let mut stack = vec![(root, 0usize)];
    while let Some((id, depth)) = stack.pop() {
        let is_dir = t.is_dir(id);
        if !is_dir && !opts.include_files {
            continue;
        }
        let s = t.dir_stats(id);
        let size_prefix = if is_dir && !t.is_exact(id) { ">=" } else { "" };
        let fields = [
            t.path(id),
            t.name(id),
            if is_dir { "Folder".into() } else { "File".into() },
            format!("{size_prefix}{}", t.logical(id)),
            format!("{size_prefix}{}", t.allocated(id)),
            s.map(|s| s.files.to_string()).unwrap_or_default(),
            s.map(|s| s.dirs.to_string()).unwrap_or_default(),
            filetime_to_iso(t.mtime(id)),
            filetime_to_iso(t.ctime(id)),
            t.ext_name(id).to_owned(),
            format!("0x{:X}", t.attrs(id)),
            flag_names(t.flags(id)).join("|"),
        ];
        for (i, f) in fields.iter().enumerate() {
            if i > 0 {
                out.write_all(b",")?;
            }
            csv_field(out, f, opts.spreadsheet_safe)?;
        }
        out.write_all(b"\r\n")?;
        if is_dir && opts.max_depth.is_none_or(|m| depth < m) {
            let kids = query::sorted_children(t, id, SortKey::Allocated, true);
            for &c in kids.iter().rev() {
                stack.push((c, depth + 1));
            }
        }
    }
    out.flush()
}

fn json_str(out: &mut impl Write, s: &str) -> io::Result<()> {
    serde_json::to_writer(&mut *out, s).map_err(io::Error::other)
}

/// Writes a nested JSON document (schema `treedock.scan`, version [`JSON_SCHEMA_VERSION`]).
pub fn write_json(t: &Tree, root: NodeId, opts: &ExportOptions, out: &mut impl Write) -> io::Result<()> {
    write!(out, "{{\"schema\":\"treedock.scan\",\"schema_version\":{JSON_SCHEMA_VERSION},\"meta\":")?;
    serde_json::to_writer(&mut *out, &t.meta).map_err(io::Error::other)?;
    out.write_all(b",\"summary\":")?;
    serde_json::to_writer(&mut *out, &query::summary(t, root)).map_err(io::Error::other)?;
    out.write_all(b",\"root\":")?;
    write_json_node(t, root, 0, opts, out)?;
    out.write_all(b"}\n")?;
    out.flush()
}

fn write_json_node(t: &Tree, id: NodeId, depth: usize, opts: &ExportOptions, out: &mut impl Write) -> io::Result<()> {
    // Iterative to survive very deep trees: an explicit stack of child cursors.
    enum Step {
        Open(NodeId, usize),
        Close,
    }
    let mut stack = vec![Step::Open(id, depth)];
    let mut first_in_level = vec![true];
    while let Some(step) = stack.pop() {
        match step {
            Step::Close => {
                out.write_all(b"]}")?;
                first_in_level.pop();
            }
            Step::Open(n, d) => {
                let level_first = first_in_level.last_mut().unwrap();
                if !*level_first {
                    out.write_all(b",")?;
                }
                *level_first = false;
                let is_dir = t.is_dir(n);
                out.write_all(b"{\"name\":")?;
                json_str(out, &t.name(n))?;
                write!(
                    out,
                    ",\"type\":\"{}\",\"logical\":{},\"allocated\":{},\"modified\":",
                    if is_dir { "dir" } else { "file" },
                    t.logical(n),
                    t.allocated(n)
                )?;
                json_str(out, &filetime_to_iso(t.mtime(n)))?;
                let fl = flag_names(t.flags(n));
                if !fl.is_empty() {
                    out.write_all(b",\"flags\":")?;
                    serde_json::to_writer(&mut *out, &fl).map_err(io::Error::other)?;
                }
                if let Some(s) = t.dir_stats(n) {
                    write!(
                        out,
                        ",\"files\":{},\"dirs\":{},\"exact\":{}",
                        s.files,
                        s.dirs,
                        s.pending == 0 && s.errors == 0
                    )?;
                    let expand = opts.max_depth.is_none_or(|m| d < m);
                    if !expand {
                        if t.has_children(n) {
                            out.write_all(b",\"truncated\":true")?;
                        }
                        out.write_all(b"}")?;
                        continue;
                    }
                    out.write_all(b",\"children\":[")?;
                    stack.push(Step::Close);
                    first_in_level.push(true);
                    let kids = query::sorted_children(t, n, SortKey::Allocated, true);
                    for &c in kids.iter().rev() {
                        if opts.include_files || t.is_dir(c) {
                            stack.push(Step::Open(c, d + 1));
                        }
                    }
                } else {
                    out.write_all(b"}")?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::tests::{dir, file, sample, w};

    #[test]
    fn csv_rows_and_escaping() {
        let mut t = Tree::new("C:\\");
        let (a, b) = (w("=evil(),\"x\".csv"), w("d"));
        t.add_children(Tree::ROOT, &[file(&a, 10), dir(&b)]).unwrap();
        t.finish_dir(Tree::ROOT, false);
        let mut buf = Vec::new();
        write_csv(&t, Tree::ROOT, &ExportOptions::default(), &mut buf).unwrap();
        let s = String::from_utf8(buf).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines.len(), 4);
        assert!(lines[0].starts_with("Path,Name,Type"));
        assert!(s.contains("\"'=evil(),\"\"x\"\".csv\""), "{s}");
        // Pending subfolder makes root total inexact.
        assert!(lines[1].contains(",>=10,"), "{}", lines[1]);
    }

    #[test]
    fn json_is_valid_and_respects_depth() {
        let t = sample();
        let mut buf = Vec::new();
        write_json(&t, Tree::ROOT, &ExportOptions::default(), &mut buf).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(v["schema_version"], 1);
        assert_eq!(v["root"]["logical"], 5100);
        assert_eq!(v["root"]["children"][0]["name"], "sub");
        assert_eq!(v["root"]["children"][0]["children"].as_array().unwrap().len(), 2);

        let mut buf = Vec::new();
        let opts = ExportOptions { max_depth: Some(0), ..Default::default() };
        write_json(&t, Tree::ROOT, &opts, &mut buf).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(v["root"]["truncated"], true);
        assert!(v["root"].get("children").is_none());

        let mut buf = Vec::new();
        let opts = ExportOptions { include_files: false, ..Default::default() };
        write_json(&t, Tree::ROOT, &opts, &mut buf).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&buf).unwrap();
        assert_eq!(v["root"]["children"].as_array().unwrap().len(), 2);
    }
}
