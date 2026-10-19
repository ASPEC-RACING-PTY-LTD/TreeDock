// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! `treedock-cli`: read-only scanner and exporter (MASTER_PLAN §4.11).
//! It never modifies, moves or deletes files.

use std::io::{self, BufWriter, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use treedock_core::export::{self, ExportOptions};
use treedock_core::fmt::{count, size_binary};
use treedock_core::query::{self, SizeMeasure};
use treedock_core::scan::{Progress, ScanControl, WalkOutcome};
use treedock_core::{Tree, snapshot};
use treedock_win::volume::{self, Media};

#[derive(Parser)]
#[command(name = "treedock-cli", version, about = "TreeDock disk space analyser (read-only command line)")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Csv,
    Json,
}

#[derive(Clone, Copy, ValueEnum)]
enum MediaArg {
    Ssd,
    Hdd,
    Removable,
    Network,
}

#[derive(Clone, Copy, ValueEnum)]
enum ModeArg {
    Auto,
    Walk,
    Mft,
}

#[derive(Clone, Copy, ValueEnum)]
enum LinkArg {
    /// Count once, at the first path found.
    Once,
    /// Split equally across links.
    Divide,
    /// Full size at every link (totals may exceed used space).
    Every,
}

#[derive(Clone, Copy, ValueEnum)]
enum By {
    Allocated,
    Logical,
}

#[derive(clap::Args)]
struct ScanArgs {
    /// Scanner: auto uses the fast MFT scan when elevated on a local NTFS volume root.
    #[arg(long, value_enum, default_value = "auto")]
    mode: ModeArg,
    /// How hard-linked files are counted (MFT scan only).
    #[arg(long, value_enum, default_value = "once")]
    hardlinks: LinkArg,
    /// Number of walker threads (default depends on the storage type).
    #[arg(long)]
    threads: Option<usize>,
    /// Override detected storage type (affects default parallelism).
    #[arg(long, value_enum)]
    media: Option<MediaArg>,
    /// Lower CPU and I/O priority while scanning.
    #[arg(long)]
    low_impact: bool,
    /// Suppress the progress line.
    #[arg(long, short)]
    quiet: bool,
}

#[derive(Subcommand)]
enum Cmd {
    /// List volumes with filesystem, capacity and storage type.
    Volumes {
        #[arg(long)]
        json: bool,
    },
    /// Scan a folder or drive and print a summary.
    Scan {
        path: String,
        #[command(flatten)]
        scan: ScanArgs,
        /// Save the scan as a .tdsnap snapshot.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Show the N largest files.
        #[arg(long, default_value_t = 10)]
        top: usize,
        /// Show the N largest file types.
        #[arg(long, default_value_t = 10)]
        types: usize,
        /// Print the summary as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Export a snapshot (or a fresh scan of a folder) as CSV or JSON.
    Export {
        /// A .tdsnap file or a folder to scan.
        input: String,
        #[arg(long, value_enum, default_value = "csv")]
        format: Format,
        /// Output file (default: standard output).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Maximum depth below the root.
        #[arg(long)]
        depth: Option<usize>,
        /// Export folders only.
        #[arg(long)]
        dirs_only: bool,
        /// CSV: do not neutralise formula-like cells.
        #[arg(long)]
        raw: bool,
        /// CSV: write a UTF-8 BOM for Excel.
        #[arg(long)]
        bom: bool,
        #[command(flatten)]
        scan: ScanArgs,
    },
    /// Largest files in a snapshot or folder.
    Top {
        input: String,
        #[arg(short, default_value_t = 50)]
        n: usize,
        #[arg(long, value_enum, default_value = "allocated")]
        by: By,
        #[command(flatten)]
        scan: ScanArgs,
    },
    /// Per-extension totals for a snapshot or folder.
    Types {
        input: String,
        #[arg(short, default_value_t = 30)]
        n: usize,
        #[command(flatten)]
        scan: ScanArgs,
    },
    /// Show snapshot metadata and validate its structure.
    Info { snapshot: PathBuf },
    /// Benchmark scanning: repeated runs, median time, memory per entry (MASTER_PLAN §9).
    Bench {
        path: String,
        #[arg(long, default_value_t = 3)]
        runs: usize,
        #[command(flatten)]
        scan: ScanArgs,
        /// Print results as JSON.
        #[arg(long)]
        json: bool,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("treedock-cli: {e}");
            ExitCode::FAILURE
        }
    }
}

type Res<T> = Result<T, String>;

fn run(cli: Cli) -> Res<()> {
    match cli.cmd {
        Cmd::Volumes { json } => cmd_volumes(json),
        Cmd::Scan { path, scan, out, top, types, json } => {
            let (tree, outcome) = scan_path(&path, &scan)?;
            if let Some(out) = out {
                snapshot::save(&tree, &out).map_err(|e| e.to_string())?;
                if !scan.quiet {
                    eprintln!("Saved snapshot to {}", out.display());
                }
            }
            if json {
                print_json_summary(&tree, &outcome, top, types)
            } else {
                print_summary(&tree, &outcome, top, types);
                Ok(())
            }
        }
        Cmd::Export { input, format, out, depth, dirs_only, raw, bom, scan } => {
            let tree = load_or_scan(&input, &scan)?;
            let opts = ExportOptions { max_depth: depth, include_files: !dirs_only, spreadsheet_safe: !raw, bom };
            let mut w: Box<dyn Write> = match &out {
                Some(p) => {
                    Box::new(BufWriter::new(std::fs::File::create(p).map_err(|e| format!("{}: {e}", p.display()))?))
                }
                None => Box::new(BufWriter::new(io::stdout().lock())),
            };
            match format {
                Format::Csv => export::write_csv(&tree, Tree::ROOT, &opts, &mut w),
                Format::Json => export::write_json(&tree, Tree::ROOT, &opts, &mut w),
            }
            .map_err(|e| e.to_string())
        }
        Cmd::Top { input, n, by, scan } => {
            let tree = load_or_scan(&input, &scan)?;
            let m = match by {
                By::Allocated => SizeMeasure::Allocated,
                By::Logical => SizeMeasure::Logical,
            };
            for id in query::top_files(&tree, Tree::ROOT, n, m) {
                println!("{:>12}  {}", size_binary(m.of(&tree, id)), tree.path(id));
            }
            Ok(())
        }
        Cmd::Types { input, n, scan } => {
            let tree = load_or_scan(&input, &scan)?;
            print_types(&tree, n);
            Ok(())
        }
        Cmd::Bench { path, runs, mut scan, json } => cmd_bench(&path, runs.max(1), &mut scan, json),
        Cmd::Info { snapshot: p } => {
            let t = snapshot::load(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            println!("{}", serde_json::to_string_pretty(&t.meta).unwrap());
            println!("nodes: {}  (structure and aggregates verified)", count(t.len() as u64));
            Ok(())
        }
    }
}

fn cmd_volumes(json: bool) -> Res<()> {
    let vols = volume::list_volumes();
    if json {
        let v: Vec<serde_json::Value> = vols
            .iter()
            .map(|v| {
                serde_json::json!({
                    "root": v.root, "label": v.label, "filesystem": v.filesystem,
                    "serial": format!("{:08X}", v.serial), "cluster_size": v.cluster_size,
                    "total": v.total, "free": v.free,
                    "drive_type": volume::drive_type_name(v.drive_type),
                    "media": v.media.map(|m| m.as_str()),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
        return Ok(());
    }
    println!(
        "{:<6} {:<8} {:>10} {:>10} {:>6}  {:<10} {:<9} Label",
        "Root", "FS", "Total", "Free", "Used", "Type", "Media"
    );
    for v in vols {
        let used = (100 * (v.total - v.free)).checked_div(v.total).unwrap_or(0);
        println!(
            "{:<6} {:<8} {:>10} {:>10} {:>5}%  {:<10} {:<9} {}",
            v.root,
            v.filesystem,
            size_binary(v.total),
            size_binary(v.free),
            used,
            volume::drive_type_name(v.drive_type),
            v.media.map(|m| m.as_str()).unwrap_or("?"),
            v.label
        );
    }
    println!("\nElevated: {}", if volume::is_elevated() { "yes" } else { "no (standard scan mode)" });
    Ok(())
}

fn scan_path(path: &str, args: &ScanArgs) -> Res<(Tree, WalkOutcome)> {
    let media = args.media.map(|m| match m {
        MediaArg::Ssd => Media::Ssd,
        MediaArg::Hdd => Media::Hdd,
        MediaArg::Removable => Media::Removable,
        MediaArg::Network => Media::Network,
    });
    let opts = treedock_win::ScanOptions {
        media,
        threads: args.threads,
        low_impact: args.low_impact,
        mode: match args.mode {
            ModeArg::Auto => treedock_win::ScanMode::Auto,
            ModeArg::Walk => treedock_win::ScanMode::Walk,
            ModeArg::Mft => treedock_win::ScanMode::Mft,
        },
        hardlinks: match args.hardlinks {
            LinkArg::Once => treedock_ntfs::HardlinkPolicy::Once,
            LinkArg::Divide => treedock_ntfs::HardlinkPolicy::Divide,
            LinkArg::Every => treedock_ntfs::HardlinkPolicy::Every,
        },
    };
    let setup = treedock_win::prepare(path, &opts)?;
    let tree = Arc::new(RwLock::new(treedock_win::new_tree(&setup)));
    let progress = Progress::new();
    let control = ScanControl::new();
    let show = !args.quiet && io::stderr().is_terminal();
    let reporter = show.then(|| {
        let p = progress.clone();
        std::thread::spawn(move || {
            let mut last_len = 0usize;
            while !p.done.load(std::sync::atomic::Ordering::Acquire) {
                let s = p.snapshot();
                let mut cur = s.current;
                if cur.chars().count() > 60 {
                    cur = format!(
                        "…{}",
                        cur.chars().rev().take(59).collect::<Vec<_>>().into_iter().rev().collect::<String>()
                    );
                }
                let line = format!(
                    "\r{} files, {} folders, {} | {}",
                    count(s.files),
                    count(s.dirs),
                    size_binary(s.allocated),
                    cur
                );
                let len = line.chars().count();
                eprint!("{line}{}", " ".repeat(last_len.saturating_sub(len)));
                last_len = len;
                std::thread::sleep(Duration::from_millis(200));
            }
            eprint!("\r{}\r", " ".repeat(last_len));
        })
    });
    if !args.quiet {
        eprintln!("Scanning {} ({}, {} threads)…", setup.display, setup.media.as_str(), setup.threads);
    }
    let outcome = treedock_win::run(&setup, tree.clone(), control, progress);
    if let Some(r) = reporter {
        let _ = r.join();
    }
    if matches!(args.mode, ModeArg::Mft) && outcome.cancelled && !outcome.errors.is_empty() {
        return Err(format!("fast MFT scan unavailable: {}", outcome.errors[0].message));
    }
    let tree = Arc::try_unwrap(tree).map_err(|_| "scan still referenced")?.into_inner().map_err(|e| e.to_string())?;
    Ok((tree, outcome))
}

fn load_or_scan(input: &str, args: &ScanArgs) -> Res<Tree> {
    let p = Path::new(input);
    if p.is_file() {
        return snapshot::load(p).map_err(|e| format!("{input}: {e}"));
    }
    Ok(scan_path(input, args)?.0)
}

fn print_summary(t: &Tree, out: &WalkOutcome, top: usize, types: usize) {
    let s = query::summary(t, Tree::ROOT);
    let ge = if t.is_exact(Tree::ROOT) { "" } else { "≥ " };
    println!("{}", s.root);
    println!("  Size (logical):    {ge}{} ({} bytes)", size_binary(s.logical), count(s.logical));
    println!("  Size on disk:      {ge}{} ({} bytes)", size_binary(s.allocated), count(s.allocated));
    println!("  Files / folders:   {} / {}", count(s.files), count(s.dirs));
    println!(
        "  Scan:              {} in {:.2?}{}",
        t.meta.scan_mode,
        out.elapsed,
        if out.cancelled { " (CANCELLED)" } else { "" }
    );
    if t.meta.volume_total > 0 {
        let used = t.meta.volume_total - t.meta.volume_free;
        println!(
            "  Volume:            {} {} used of {} ({}), cluster {}",
            t.meta.filesystem,
            size_binary(used),
            size_binary(t.meta.volume_total),
            t.meta.volume_label,
            size_binary(t.meta.cluster_size as u64)
        );
    }
    if s.cloud_files > 0 {
        println!(
            "  Cloud placeholders: {} files, {} not stored locally",
            count(s.cloud_files),
            size_binary(s.cloud_logical)
        );
    }
    if s.links_not_followed > 0 {
        println!("  Links not followed: {} (junctions/symlinks/mount points)", count(s.links_not_followed));
    }
    if s.estimated_files > 0 {
        println!("  Estimated sizes:   {} files", count(s.estimated_files));
    }
    for n in &t.meta.notes {
        println!("  Note: {n}");
    }
    if !out.errors.is_empty() {
        println!("  Inaccessible:      {} folders (totals are lower bounds)", out.errors.len());
        for e in out.errors.iter().take(5) {
            println!("    {} — {}", e.path, e.message.trim());
        }
        if out.errors.len() > 5 {
            println!("    … and {} more", out.errors.len() - 5);
        }
    }
    if top > 0 {
        println!("\nLargest folders:");
        for id in query::sorted_children(t, Tree::ROOT, query::SortKey::Allocated, true).into_iter().take(top) {
            let pct = if s.allocated > 0 { 100.0 * t.allocated(id) as f64 / s.allocated as f64 } else { 0.0 };
            println!(
                "  {:>12} {:>5.1}%  {}{}",
                size_binary(t.allocated(id)),
                pct,
                t.name(id),
                if t.is_dir(id) { "\\" } else { "" }
            );
        }
        println!("\nLargest files:");
        for id in query::top_files(t, Tree::ROOT, top, SizeMeasure::Allocated) {
            println!("  {:>12}  {}", size_binary(t.allocated(id)), t.path(id));
        }
    }
    if types > 0 {
        println!();
        print_types(t, types);
    }
}

fn print_types(t: &Tree, n: usize) {
    println!("{:<12} {:>12} {:>12} {:>7}", "Type", "On disk", "Files", "Share");
    let total = t.allocated(Tree::ROOT).max(1) as f64;
    for e in query::ext_stats(t, Tree::ROOT).into_iter().take(n) {
        let name = if e.ext.is_empty() { "(none)".to_string() } else { format!(".{}", e.ext) };
        println!(
            "{:<12} {:>12} {:>12} {:>6.1}%",
            name,
            size_binary(e.allocated),
            count(e.files),
            100.0 * e.allocated as f64 / total
        );
    }
}

fn print_json_summary(t: &Tree, out: &WalkOutcome, top: usize, types: usize) -> Res<()> {
    let v = serde_json::json!({
        "meta": t.meta,
        "summary": query::summary(t, Tree::ROOT),
        "exact": t.is_exact(Tree::ROOT),
        "elapsed_ms": out.elapsed.as_millis() as u64,
        "cancelled": out.cancelled,
        "errors": out.errors.iter().map(|e| serde_json::json!({"path": e.path, "code": e.code, "message": e.message.trim()})).collect::<Vec<_>>(),
        "top_files": query::top_files(t, Tree::ROOT, top, SizeMeasure::Allocated).into_iter()
            .map(|id| serde_json::json!({"path": t.path(id), "allocated": t.allocated(id), "logical": t.logical(id)})).collect::<Vec<_>>(),
        "types": query::ext_stats(t, Tree::ROOT).into_iter().take(types).collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?);
    Ok(())
}

fn cmd_bench(path: &str, runs: usize, args: &mut ScanArgs, json: bool) -> Res<()> {
    args.quiet = true;
    let mut times = Vec::with_capacity(runs);
    let mut last = None;
    for i in 0..runs {
        let (t, out) = scan_path(path, args)?;
        times.push(out.elapsed);
        if !json {
            eprintln!("run {}: {:.3?} ({} entries)", i + 1, out.elapsed, count(t.len() as u64));
        }
        last = Some(t);
    }
    let t = last.unwrap();
    times.sort();
    let median = times[times.len() / 2];
    let entries = t.len() as u64;
    let mem = t.memory_bytes() as u64;
    let peak = volume::peak_working_set();
    let v = serde_json::json!({
        "path": t.meta.root_path, "mode": t.meta.scan_mode, "runs": runs,
        "median_ms": median.as_secs_f64() * 1000.0,
        "min_ms": times[0].as_secs_f64() * 1000.0, "max_ms": times[times.len() - 1].as_secs_f64() * 1000.0,
        "entries": entries, "entries_per_sec": entries as f64 / median.as_secs_f64().max(1e-9),
        "tree_bytes": mem, "tree_bytes_per_entry": mem as f64 / entries.max(1) as f64,
        "peak_working_set": peak, "exact": t.is_exact(Tree::ROOT),
        "note": "Cache state is not controlled: the first run is usually cold, later runs warm.",
    });
    if json {
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
    } else {
        println!("{} — {}", t.meta.root_path, t.meta.scan_mode);
        println!("  median {:.3?} (min {:.3?}, max {:.3?}) over {runs} runs", median, times[0], times[times.len() - 1]);
        println!("  {} entries, {:.0} entries/s", count(entries), entries as f64 / median.as_secs_f64().max(1e-9));
        println!(
            "  tree store {} ({:.1} bytes/entry), peak working set {}",
            size_binary(mem),
            mem as f64 / entries.max(1) as f64,
            size_binary(peak)
        );
    }
    Ok(())
}
