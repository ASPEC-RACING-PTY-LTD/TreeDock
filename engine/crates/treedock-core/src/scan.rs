// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Parallel directory-walk engine (MASTER_PLAN §5.3, §6.2).
//!
//! Workers enumerate directories through a platform-specific [`DirEnumerator`]
//! and send compact batches to a single builder (the calling thread), which is
//! the only writer of the [`Tree`]. Readers (UI paging, progress) take short
//! read locks. Cancellation is cooperative and the builder never waits for a
//! stuck worker: workers are detached and abandonable.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::time::{Duration, Instant};

use crossbeam_channel as chan;

use crate::tree::{NewNode, NodeId, Tree, attr, flags, reparse};

/// One directory entry as reported by the filesystem.
#[derive(Clone, Debug, Default)]
pub struct RawEntry {
    pub name: Vec<u16>,
    pub attrs: u32,
    pub logical: u64,
    /// `None` when the enumeration API does not report allocation.
    pub allocated: Option<u64>,
    pub mtime: i64,
    pub ctime: i64,
    pub file_id: u64,
    /// Valid only when `attrs` has `REPARSE_POINT`.
    pub reparse_tag: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnumError {
    /// OS error code (Win32 error on Windows).
    pub code: u32,
    pub message: String,
}

/// Enumerates a single directory. Implementations must be thread-safe.
pub trait DirEnumerator: Send + Sync + 'static {
    type Path: Clone + Send + 'static;

    /// Appends the entries of `path` to `out` (excluding `.` and `..` is
    /// optional; the engine filters them). On error, entries already appended
    /// are kept and the directory is marked as having an error.
    fn enumerate(&self, path: &Self::Path, out: &mut Vec<RawEntry>) -> Result<(), EnumError>;

    fn join(&self, parent: &Self::Path, name: &[u16]) -> Self::Path;

    fn display(&self, path: &Self::Path) -> String;

    /// Called once on each worker thread before it starts (e.g. to lower I/O priority).
    fn on_worker_start(&self) {}
}

#[derive(Clone, Debug)]
pub struct WalkOptions {
    /// Number of enumeration worker threads.
    pub threads: usize,
    /// Cluster size used to estimate allocation when the API does not report it.
    pub cluster_size: u32,
}

impl Default for WalkOptions {
    fn default() -> Self {
        WalkOptions { threads: 8, cluster_size: 4096 }
    }
}

/// Cancellation and pause control shared between the UI and the scan.
#[derive(Debug, Default)]
pub struct ScanControl {
    cancelled: AtomicBool,
    paused: Mutex<bool>,
    cv: Condvar,
}

impl ScanControl {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        // Wake paused workers so they observe cancellation.
        self.resume();
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    pub fn pause(&self) {
        *self.paused.lock().unwrap() = true;
    }

    pub fn resume(&self) {
        *self.paused.lock().unwrap() = false;
        self.cv.notify_all();
    }

    pub fn is_paused(&self) -> bool {
        *self.paused.lock().unwrap()
    }

    fn wait_if_paused(&self) {
        let mut p = self.paused.lock().unwrap();
        while *p && !self.is_cancelled() {
            p = self.cv.wait(p).unwrap();
        }
    }
}

/// Live counters, safe to poll from any thread.
#[derive(Debug, Default)]
pub struct Progress {
    pub files: AtomicU64,
    pub dirs: AtomicU64,
    pub logical: AtomicU64,
    pub allocated: AtomicU64,
    pub errors: AtomicU64,
    pub done: AtomicBool,
    current: Mutex<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProgressSnapshot {
    pub files: u64,
    pub dirs: u64,
    pub logical: u64,
    pub allocated: u64,
    pub errors: u64,
    pub done: bool,
    pub current: String,
}

impl Progress {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn snapshot(&self) -> ProgressSnapshot {
        ProgressSnapshot {
            files: self.files.load(Ordering::Relaxed),
            dirs: self.dirs.load(Ordering::Relaxed),
            logical: self.logical.load(Ordering::Relaxed),
            allocated: self.allocated.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            done: self.done.load(Ordering::Acquire),
            current: self.current.lock().map(|s| s.clone()).unwrap_or_default(),
        }
    }

    fn set_current(&self, s: String) {
        if let Ok(mut c) = self.current.try_lock() {
            *c = s;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanError {
    pub path: String,
    pub code: u32,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct WalkOutcome {
    pub errors: Vec<ScanError>,
    pub cancelled: bool,
    pub elapsed: Duration,
}

struct Job<P> {
    node: NodeId,
    path: P,
}

#[derive(Default)]
struct EntryRec {
    name_start: u32,
    name_len: u32,
    attrs: u32,
    logical: u64,
    allocated: u64,
    mtime: i64,
    ctime: i64,
    file_id: u64,
    flags: u16,
}

struct Batch<P> {
    node: NodeId,
    names: Vec<u16>,
    entries: Vec<EntryRec>,
    /// Paths of entries flagged `PENDING`, in entry order.
    subpaths: Vec<P>,
    error: Option<ScanError>,
    /// Job was skipped because of cancellation.
    skipped: bool,
}

/// Classifies a raw entry into tree flags and sizes.
pub fn classify(e: &RawEntry, cluster_size: u32) -> (u16, u64) {
    let mut f = 0u16;
    let is_dir = e.attrs & attr::DIRECTORY != 0;
    if is_dir {
        f |= flags::DIR;
    }
    if e.attrs & attr::REPARSE_POINT != 0 {
        f |= flags::REPARSE;
        if reparse::is_name_surrogate(e.reparse_tag) {
            f |= flags::SURROGATE;
        }
        if reparse::is_cloud(e.reparse_tag) {
            f |= flags::CLOUD;
        }
    }
    if e.attrs & (attr::RECALL_ON_OPEN | attr::RECALL_ON_DATA_ACCESS) != 0 {
        f |= flags::CLOUD;
    }
    if e.attrs & attr::COMPRESSED != 0 {
        f |= flags::COMPRESSED;
    }
    if e.attrs & attr::SPARSE_FILE != 0 {
        f |= flags::SPARSE;
    }
    if e.attrs & attr::OFFLINE != 0 {
        f |= flags::OFFLINE;
    }
    if is_dir && f & flags::SURROGATE == 0 {
        f |= flags::PENDING;
    }
    let allocated = if is_dir {
        0
    } else {
        match e.allocated {
            Some(a) => a,
            None => {
                f |= flags::ESTIMATED;
                let c = cluster_size.max(1) as u64;
                e.logical.div_ceil(c) * c
            }
        }
    };
    (f, allocated)
}

fn is_dot(name: &[u16]) -> bool {
    matches!(name, [0x2E] | [0x2E, 0x2E])
}

/// Runs a parallel walk of `root_path` into `tree` (whose root node must be
/// pending). Blocks the calling thread, which acts as the single tree writer.
pub fn walk<E: DirEnumerator>(
    enumerator: Arc<E>,
    root_path: E::Path,
    tree: Arc<RwLock<Tree>>,
    opts: &WalkOptions,
    control: Arc<ScanControl>,
    progress: Arc<Progress>,
) -> WalkOutcome {
    let start = Instant::now();
    let threads = opts.threads.max(1);
    let (job_tx, job_rx) = chan::unbounded::<Job<E::Path>>();
    let (res_tx, res_rx) = chan::bounded::<Batch<E::Path>>(threads * 8);

    for i in 0..threads {
        let job_rx = job_rx.clone();
        let res_tx = res_tx.clone();
        let enumerator = enumerator.clone();
        let control = control.clone();
        let progress = progress.clone();
        let cluster = opts.cluster_size;
        std::thread::Builder::new()
            .name(format!("treedock-walk-{i}"))
            .spawn(move || worker(job_rx, res_tx, enumerator, control, progress, cluster))
            .expect("spawn walk worker");
    }
    drop(res_tx);

    let mut outstanding = 1usize;
    job_tx.send(Job { node: Tree::ROOT, path: root_path }).expect("queue root");
    let mut errors = Vec::new();
    let mut cancelled = false;

    while outstanding > 0 {
        if control.is_cancelled() {
            cancelled = true;
            break;
        }
        let batch = match res_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(b) => b,
            Err(chan::RecvTimeoutError::Timeout) => continue,
            Err(chan::RecvTimeoutError::Disconnected) => break,
        };
        outstanding -= 1;
        if batch.skipped {
            continue;
        }
        let nodes: Vec<NewNode> = batch
            .entries
            .iter()
            .map(|e| NewNode {
                name: &batch.names[e.name_start as usize..(e.name_start + e.name_len) as usize],
                attrs: e.attrs,
                logical: e.logical,
                allocated: e.allocated,
                mtime: e.mtime,
                ctime: e.ctime,
                file_id: e.file_id,
                flags: e.flags,
            })
            .collect();
        let new_dirs = {
            let mut t = tree.write().unwrap();
            let r = t.add_children(batch.node, &nodes);
            t.finish_dir(batch.node, batch.error.is_some() || r.is_err());
            match r {
                Ok(d) => d,
                Err(e) => {
                    errors.push(ScanError { path: t.path(batch.node), code: 0, message: e.to_string() });
                    cancelled = true;
                    break;
                }
            }
        };
        if let Some(e) = batch.error {
            errors.push(e);
        }
        debug_assert_eq!(new_dirs.len(), batch.subpaths.len());
        for (node, path) in new_dirs.into_iter().zip(batch.subpaths) {
            outstanding += 1;
            let _ = job_tx.send(Job { node, path });
        }
    }
    // Dropping the job sender lets idle workers exit; busy workers are abandoned.
    drop(job_tx);
    if cancelled {
        control.cancel();
    }
    progress.done.store(true, Ordering::Release);
    WalkOutcome { errors, cancelled, elapsed: start.elapsed() }
}

fn worker<E: DirEnumerator>(
    jobs: chan::Receiver<Job<E::Path>>,
    results: chan::Sender<Batch<E::Path>>,
    enumerator: Arc<E>,
    control: Arc<ScanControl>,
    progress: Arc<Progress>,
    cluster_size: u32,
) {
    enumerator.on_worker_start();
    let mut raw = Vec::with_capacity(256);
    let mut counter = 0u32;
    while let Ok(job) = jobs.recv() {
        control.wait_if_paused();
        if control.is_cancelled() {
            let skip = Batch {
                node: job.node,
                names: Vec::new(),
                entries: Vec::new(),
                subpaths: Vec::new(),
                error: None,
                skipped: true,
            };
            if results.send(skip).is_err() {
                return;
            }
            continue;
        }
        counter = counter.wrapping_add(1);
        if counter % 32 == 1 {
            progress.set_current(enumerator.display(&job.path));
        }
        raw.clear();
        let res = enumerator.enumerate(&job.path, &mut raw);
        let mut batch = Batch {
            node: job.node,
            names: Vec::new(),
            entries: Vec::with_capacity(raw.len()),
            subpaths: Vec::new(),
            error: None,
            skipped: false,
        };
        let (mut files, mut dirs, mut logical, mut allocated) = (0u64, 0u64, 0u64, 0u64);
        for e in raw.drain(..) {
            if is_dot(&e.name) {
                continue;
            }
            let (f, alloc) = classify(&e, cluster_size);
            if f & flags::DIR != 0 {
                dirs += 1;
            } else {
                files += 1;
                logical += e.logical;
                allocated += alloc;
            }
            if f & flags::PENDING != 0 {
                batch.subpaths.push(enumerator.join(&job.path, &e.name));
            }
            batch.entries.push(EntryRec {
                name_start: batch.names.len() as u32,
                name_len: e.name.len() as u32,
                attrs: e.attrs,
                logical: e.logical,
                allocated: alloc,
                mtime: e.mtime,
                ctime: e.ctime,
                file_id: e.file_id,
                flags: f,
            });
            batch.names.extend_from_slice(&e.name);
        }
        if let Err(err) = res {
            progress.errors.fetch_add(1, Ordering::Relaxed);
            batch.error = Some(ScanError { path: enumerator.display(&job.path), code: err.code, message: err.message });
        }
        progress.files.fetch_add(files, Ordering::Relaxed);
        progress.dirs.fetch_add(dirs, Ordering::Relaxed);
        progress.logical.fetch_add(logical, Ordering::Relaxed);
        progress.allocated.fetch_add(allocated, Ordering::Relaxed);
        if results.send(batch).is_err() {
            return;
        }
    }
}

/// An in-memory filesystem used by tests and benchmarks.
pub mod memfs {
    use super::*;
    use std::collections::HashMap;

    #[derive(Clone, Debug, Default)]
    pub struct MemFs {
        /// Directory path ("" = root, components joined with '\') → entries.
        pub dirs: HashMap<String, Vec<RawEntry>>,
        /// Paths whose enumeration fails with the given code.
        pub fail: HashMap<String, u32>,
        /// Artificial per-directory delay.
        pub delay: Option<Duration>,
    }

    impl MemFs {
        pub fn add_dir(&mut self, path: &str) {
            self.dirs.entry(path.to_owned()).or_default();
            if let Some((parent, name)) = split(path) {
                self.add_dir(parent);
                let entries = self.dirs.get_mut(parent).unwrap();
                if !entries.iter().any(|e| e.name == name.encode_utf16().collect::<Vec<_>>()) {
                    entries.push(RawEntry {
                        name: name.encode_utf16().collect(),
                        attrs: attr::DIRECTORY,
                        ..Default::default()
                    });
                }
            }
        }

        pub fn add_file(&mut self, path: &str, size: u64) {
            self.add_entry(
                path,
                RawEntry { logical: size, allocated: Some(size.div_ceil(4096) * 4096), ..Default::default() },
            );
        }

        pub fn add_entry(&mut self, path: &str, mut e: RawEntry) {
            let (parent, name) = split(path).expect("file needs a parent");
            self.add_dir(parent);
            e.name = name.encode_utf16().collect();
            self.dirs.get_mut(parent).unwrap().push(e);
        }
    }

    fn split(path: &str) -> Option<(&str, &str)> {
        if path.is_empty() {
            return None;
        }
        Some(match path.rfind('\\') {
            Some(i) => (&path[..i], &path[i + 1..]),
            None => ("", path),
        })
    }

    impl DirEnumerator for MemFs {
        type Path = String;

        fn enumerate(&self, path: &String, out: &mut Vec<RawEntry>) -> Result<(), EnumError> {
            if let Some(d) = self.delay {
                std::thread::sleep(d);
            }
            if let Some(&code) = self.fail.get(path) {
                return Err(EnumError { code, message: "access denied".into() });
            }
            out.extend(self.dirs.get(path).cloned().unwrap_or_default());
            Ok(())
        }

        fn join(&self, parent: &String, name: &[u16]) -> String {
            let n = String::from_utf16_lossy(name);
            if parent.is_empty() { n } else { format!("{parent}\\{n}") }
        }

        fn display(&self, path: &String) -> String {
            path.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::memfs::MemFs;
    use super::*;

    fn run(fs: MemFs, threads: usize) -> (Tree, WalkOutcome) {
        let tree = Arc::new(RwLock::new(Tree::new("M:\\")));
        let out = walk(
            Arc::new(fs),
            String::new(),
            tree.clone(),
            &WalkOptions { threads, cluster_size: 4096 },
            ScanControl::new(),
            Progress::new(),
        );
        let t = Arc::try_unwrap(tree).ok().unwrap().into_inner().unwrap();
        (t, out)
    }

    fn sample_fs() -> MemFs {
        let mut fs = MemFs::default();
        for i in 0..20 {
            for j in 0..10 {
                fs.add_file(&format!("d{i}\\sub{j}\\f.bin"), (i * 10 + j) as u64 * 1000);
            }
            fs.add_file(&format!("d{i}\\top.txt"), 1);
        }
        fs.add_dir("empty");
        fs
    }

    #[test]
    fn walk_matches_expected_totals() {
        for threads in [1, 4, 16] {
            let (t, out) = run(sample_fs(), threads);
            assert!(!out.cancelled);
            assert!(out.errors.is_empty());
            let expected: u64 = (0..200u64).map(|k| k * 1000).sum::<u64>() + 20;
            assert_eq!(t.logical(Tree::ROOT), expected);
            let s = t.dir_stats(Tree::ROOT).unwrap();
            assert_eq!(s.files, 220);
            assert_eq!(s.dirs, 20 + 200 + 1);
            assert_eq!(s.pending, 0);
            t.check_invariants().unwrap();
        }
    }

    #[test]
    fn errors_are_recorded_and_scan_continues() {
        let mut fs = sample_fs();
        fs.fail.insert("d3".into(), 5);
        let (t, out) = run(fs, 4);
        assert_eq!(out.errors.len(), 1);
        assert_eq!(out.errors[0].code, 5);
        let d3 = t.resolve("d3").unwrap();
        assert!(t.flags(d3) & flags::ERROR != 0);
        assert!(!t.is_exact(Tree::ROOT));
        assert_eq!(t.dir_stats(Tree::ROOT).unwrap().errors, 1);
        t.check_invariants().unwrap();
    }

    #[test]
    fn junctions_are_not_followed_and_cloud_is_flagged() {
        let mut fs = MemFs::default();
        fs.add_file("real\\big.bin", 10_000);
        fs.add_entry(
            "link",
            RawEntry {
                attrs: attr::DIRECTORY | attr::REPARSE_POINT,
                reparse_tag: reparse::MOUNT_POINT,
                ..Default::default()
            },
        );
        // A "link" directory entry in memfs that would loop if followed.
        fs.dirs.insert(
            "link".into(),
            vec![RawEntry { name: "x".encode_utf16().collect(), logical: 99, ..Default::default() }],
        );
        fs.add_entry(
            "cloud.docx",
            RawEntry {
                logical: 5_000_000,
                allocated: Some(0),
                attrs: attr::RECALL_ON_DATA_ACCESS,
                ..Default::default()
            },
        );
        fs.add_entry("est.bin", RawEntry { logical: 1, allocated: None, ..Default::default() });
        let (t, _) = run(fs, 2);
        let link = t.resolve("link").unwrap();
        assert!(t.flags(link) & flags::SURROGATE != 0);
        assert!(!t.has_children(link));
        let cloud = t.resolve("cloud.docx").unwrap();
        assert!(t.flags(cloud) & flags::CLOUD != 0);
        assert_eq!(t.allocated(cloud), 0);
        let est = t.resolve("est.bin").unwrap();
        assert!(t.flags(est) & flags::ESTIMATED != 0);
        assert_eq!(t.allocated(est), 4096);
        assert_eq!(t.logical(Tree::ROOT), 10_000 + 5_000_000 + 1);
    }

    #[test]
    fn cancellation_is_prompt_and_leaves_consistent_partial_tree() {
        let mut fs = sample_fs();
        fs.delay = Some(Duration::from_millis(20));
        let tree = Arc::new(RwLock::new(Tree::new("M:\\")));
        let control = ScanControl::new();
        let c2 = control.clone();
        let t2 = tree.clone();
        let h = std::thread::spawn(move || {
            walk(Arc::new(fs), String::new(), t2, &WalkOptions { threads: 2, cluster_size: 4096 }, c2, Progress::new())
        });
        std::thread::sleep(Duration::from_millis(60));
        let t0 = Instant::now();
        control.cancel();
        let out = h.join().unwrap();
        assert!(out.cancelled);
        assert!(t0.elapsed() < Duration::from_millis(500));
        let t = tree.read().unwrap();
        assert!(t.dir_stats(Tree::ROOT).unwrap().pending > 0);
        t.check_invariants().unwrap();
    }

    #[test]
    fn pause_and_resume() {
        let mut fs = sample_fs();
        fs.delay = Some(Duration::from_millis(2));
        let tree = Arc::new(RwLock::new(Tree::new("M:\\")));
        let control = ScanControl::new();
        let progress = Progress::new();
        control.pause();
        let (c2, p2, t2) = (control.clone(), progress.clone(), tree.clone());
        let h = std::thread::spawn(move || {
            walk(Arc::new(fs), String::new(), t2, &WalkOptions { threads: 2, cluster_size: 4096 }, c2, p2)
        });
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(progress.snapshot().dirs, 0);
        control.resume();
        let out = h.join().unwrap();
        assert!(!out.cancelled);
        assert_eq!(tree.read().unwrap().dir_stats(Tree::ROOT).unwrap().files, 220);
    }
}
