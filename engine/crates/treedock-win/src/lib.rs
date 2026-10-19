// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Windows integration for the TreeDock engine: directory enumeration,
//! the NTFS MFT fast path, volume and media discovery, privilege handling and
//! scan orchestration.

#[cfg(windows)]
pub mod access;
#[cfg(windows)]
pub mod enumerate;
#[cfg(windows)]
pub mod fileops;
#[cfg(windows)]
pub mod mft;
#[cfg(windows)]
pub mod privilege;
pub mod util;
#[cfg(windows)]
pub mod volume;

#[cfg(windows)]
pub use scan::*;

#[cfg(windows)]
mod scan {
    use std::sync::atomic::Ordering;
    use std::sync::{Arc, RwLock};
    use std::time::{Duration, Instant};

    use treedock_core::scan::{Progress, ScanControl, ScanError, WalkOptions, WalkOutcome, walk};
    use treedock_core::time::now_unix_ms;
    use treedock_core::tree::{Tree, attr};
    use treedock_ntfs::{BuildOptions, HardlinkPolicy};
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileAttributesExW, GetFileAttributesW, GetFileExInfoStandard, INVALID_FILE_ATTRIBUTES,
        WIN32_FILE_ATTRIBUTE_DATA,
    };

    use crate::enumerate::WinEnumerator;
    use crate::util;
    use crate::volume::{self, Media, VolumeInfo};

    /// Requested scanner.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub enum ScanMode {
        /// MFT fast path when possible (elevated, local NTFS volume root), otherwise walk.
        #[default]
        Auto,
        Walk,
        /// MFT only; fails instead of falling back.
        Mft,
    }

    /// A resolved scan target.
    #[derive(Clone, Debug)]
    pub struct ScanSetup {
        /// Extended-length path used for all I/O.
        pub root_ext: Vec<u16>,
        /// Path shown to the user.
        pub display: String,
        pub volume: Option<VolumeInfo>,
        pub media: Media,
        pub threads: usize,
        pub low_impact: bool,
        pub mode: ScanMode,
        pub hardlinks: HardlinkPolicy,
    }

    impl ScanSetup {
        /// Drive letter when the target is the root of a local NTFS volume.
        pub fn mft_letter(&self) -> Option<char> {
            let v = self.volume.as_ref()?;
            let b = self.display.as_bytes();
            let is_root = b.len() == 3 && b[0].is_ascii_alphabetic() && &b[1..] == b":\\";
            (is_root && v.filesystem.eq_ignore_ascii_case("NTFS") && self.media != Media::Network)
                .then(|| b[0].to_ascii_uppercase() as char)
        }
    }

    #[derive(Clone, Debug, Default)]
    pub struct ScanOptions {
        pub media: Option<Media>,
        pub threads: Option<usize>,
        /// Lower CPU/I/O priority of scanner threads.
        pub low_impact: bool,
        pub mode: ScanMode,
        pub hardlinks: HardlinkPolicy,
    }

    /// Resolves and validates a scan target.
    pub fn prepare(path: &str, opts: &ScanOptions) -> Result<ScanSetup, String> {
        let full = volume::full_path(path)?;
        let root_ext = util::to_extended(&full);
        let a = unsafe { GetFileAttributesW(util::with_nul(&root_ext).as_ptr()) };
        if a == INVALID_FILE_ATTRIBUTES {
            return Err(format!("{path}: {}", volume::last_error_string()));
        }
        if a & attr::DIRECTORY == 0 {
            return Err(format!("{path}: not a folder"));
        }
        let vol = volume::volume_root(&full).and_then(|r| volume::volume_info(&r)).ok();
        let detected = vol.as_ref().and_then(|v| v.media).unwrap_or(Media::Unknown);
        let media = opts.media.unwrap_or(detected);
        Ok(ScanSetup {
            display: util::to_display(&root_ext),
            root_ext,
            threads: opts.threads.unwrap_or_else(|| media.default_threads()).max(1),
            volume: vol,
            media,
            low_impact: opts.low_impact,
            mode: opts.mode,
            hardlinks: opts.hardlinks,
        })
    }

    /// Creates an empty tree whose metadata describes `setup`.
    pub fn new_tree(setup: &ScanSetup) -> Tree {
        let mut t = Tree::new(&setup.display);
        let m = &mut t.meta;
        if let Some(v) = &setup.volume {
            m.filesystem = v.filesystem.clone();
            m.volume_label = v.label.clone();
            m.volume_serial = v.serial;
            m.cluster_size = v.cluster_size;
            m.volume_total = v.total;
            m.volume_free = v.free;
        }
        m.scan_mode = "walk".into();
        m.elevated = volume::is_elevated();
        m.started_ms = now_unix_ms();
        // The walk enumerates children only, so record the root's own timestamps here.
        let mut data: WIN32_FILE_ATTRIBUTE_DATA = unsafe { std::mem::zeroed() };
        let p = util::with_nul(&setup.root_ext);
        if unsafe {
            GetFileAttributesExW(
                p.as_ptr(),
                GetFileExInfoStandard,
                (&mut data as *mut WIN32_FILE_ATTRIBUTE_DATA).cast(),
            )
        } != 0
        {
            let ft = |f: windows_sys::Win32::Foundation::FILETIME| {
                ((f.dwHighDateTime as i64) << 32) | f.dwLowDateTime as i64
            };
            t.set_root_times(ft(data.ftLastWriteTime), ft(data.ftCreationTime));
        }
        t
    }

    /// Runs the scan (MFT fast path or walk). Blocks until done or cancelled.
    pub fn run(
        setup: &ScanSetup,
        tree: Arc<RwLock<Tree>>,
        control: Arc<ScanControl>,
        progress: Arc<Progress>,
    ) -> WalkOutcome {
        let elevated = volume::is_elevated();
        if setup.mode != ScanMode::Walk {
            let reason = match (setup.mft_letter(), elevated) {
                (Some(letter), true) => match run_mft(setup, letter, &tree, &control, &progress) {
                    Ok(out) => return out,
                    Err(e) => e,
                },
                (None, _) => "the target is not the root of a local NTFS volume".to_string(),
                (_, false) => "administrator rights are required".to_string(),
            };
            if setup.mode == ScanMode::Mft {
                progress.done.store(true, Ordering::Release);
                tree.write().unwrap().meta.notes.push(format!("Fast MFT scan unavailable: {reason}."));
                return WalkOutcome {
                    errors: vec![ScanError { path: setup.display.clone(), code: 0, message: reason }],
                    cancelled: true,
                    elapsed: Duration::ZERO,
                };
            }
            if setup.mft_letter().is_some() {
                tree.write().unwrap().meta.notes.push(format!("Fast MFT scan not used: {reason}."));
            }
        }
        run_walk(setup, tree, control, progress, elevated)
    }

    fn run_walk(
        setup: &ScanSetup,
        tree: Arc<RwLock<Tree>>,
        control: Arc<ScanControl>,
        progress: Arc<Progress>,
        elevated: bool,
    ) -> WalkOutcome {
        // With administrator rights, backup privilege lets the walk read access-denied folders.
        let _backup = if elevated { crate::privilege::BackupPrivilege::enable() } else { None };
        let enumerator = Arc::new(WinEnumerator::new(setup.low_impact));
        let cluster = setup.volume.as_ref().map(|v| v.cluster_size).filter(|&c| c > 0).unwrap_or(4096);
        let opts = WalkOptions { threads: setup.threads, cluster_size: cluster };
        let out = walk(enumerator.clone(), setup.root_ext.clone(), tree.clone(), &opts, control, progress);
        let mut t = tree.write().unwrap();
        t.meta.finished_ms = now_unix_ms();
        t.meta.complete = !out.cancelled;
        t.meta.scan_mode =
            format!("walk ({}, {} threads, {})", enumerator.mode_name(), setup.threads, setup.media.as_str());
        if enumerator.allocation_estimated() {
            t.meta.notes.push("Allocated sizes are estimated: the filesystem did not report allocation.".into());
        }
        t.meta.notes.push("Hard links are not deduplicated by the walk scanner (use the fast MFT scan).".into());
        t.shrink_to_fit();
        out
    }

    fn run_mft(
        setup: &ScanSetup,
        letter: char,
        tree: &Arc<RwLock<Tree>>,
        control: &Arc<ScanControl>,
        progress: &Arc<Progress>,
    ) -> Result<WalkOutcome, String> {
        let start = Instant::now();
        let opts = BuildOptions { hardlinks: setup.hardlinks, include_ads: true };
        let (mut built, stats, _geo) = crate::mft::scan_volume(letter, &setup.display, &opts, control, progress)?;
        let mut t = tree.write().unwrap();
        built.meta = std::mem::take(&mut t.meta);
        built.meta.finished_ms = now_unix_ms();
        built.meta.complete = true;
        built.meta.scan_mode = format!("mft ({} records, hard links: {:?})", stats.records_in_use, setup.hardlinks);
        if stats.bad_records > 0 {
            built.meta.notes.push(format!("{} MFT records could not be parsed and were skipped.", stats.bad_records));
        }
        if stats.build.orphans > 0 {
            built
                .meta
                .notes
                .push(format!("{} entries with broken parent links are listed under $Orphaned.", stats.build.orphans));
        }
        built.meta.notes.push(format!("{} files have multiple hard links.", stats.build.hardlinked_files));
        built.shrink_to_fit();
        let s = built.dir_stats(Tree::ROOT).unwrap_or_default();
        progress.files.store(s.files, Ordering::Relaxed);
        progress.dirs.store(s.dirs, Ordering::Relaxed);
        progress.allocated.store(built.allocated(Tree::ROOT), Ordering::Relaxed);
        progress.logical.store(built.logical(Tree::ROOT), Ordering::Relaxed);
        *t = built;
        progress.done.store(true, Ordering::Release);
        Ok(WalkOutcome { errors: Vec::new(), cancelled: false, elapsed: start.elapsed() })
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use treedock_core::tree::flags;

        #[test]
        fn scan_real_directory() {
            let d = std::env::temp_dir().join(format!("treedock-scan-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&d);
            std::fs::create_dir_all(d.join("a\\b\\c")).unwrap();
            std::fs::write(d.join("a\\one.bin"), vec![0u8; 10_000]).unwrap();
            std::fs::write(d.join("a\\b\\c\\two.TXT"), vec![0u8; 123]).unwrap();
            std::fs::create_dir_all(d.join("empty")).unwrap();
            // A deep path beyond MAX_PATH exercises the \\?\ handling.
            let mut cur = util::to_extended(&util::wide(d.to_str().unwrap()));
            for _ in 0..30 {
                cur = util::join(&cur, &util::wide("deep_directory_name"));
                let p = util::with_nul(&cur);
                unsafe { windows_sys::Win32::Storage::FileSystem::CreateDirectoryW(p.as_ptr(), std::ptr::null()) };
            }
            assert!(cur.len() > 300);

            let setup = prepare(d.to_str().unwrap(), &ScanOptions { threads: Some(4), ..Default::default() }).unwrap();
            assert!(setup.mft_letter().is_none());
            let tree = Arc::new(RwLock::new(new_tree(&setup)));
            let out = run(&setup, tree.clone(), ScanControl::new(), Progress::new());
            assert!(out.errors.is_empty(), "{:?}", out.errors);
            let t = tree.read().unwrap();
            t.check_invariants().unwrap();
            assert_eq!(t.logical(Tree::ROOT), 10_123);
            let s = t.dir_stats(Tree::ROOT).unwrap();
            assert_eq!(s.files, 2);
            assert_eq!(s.dirs, 4 + 30);
            let two = t.resolve("a\\b\\c\\two.txt").unwrap();
            assert_eq!(t.ext_name(two), "txt");
            assert_eq!(t.flags(two) & flags::ESTIMATED, 0);
            assert!(t.meta.complete);
            assert!(t.mtime(Tree::ROOT) > 0);
            drop(t);
            let _ = std::fs::remove_dir_all(format!(r"\\?\{}", d.display()));
        }

        #[test]
        fn mft_only_mode_reports_why_it_is_unavailable() {
            let d = std::env::temp_dir();
            let opts = ScanOptions { mode: ScanMode::Mft, ..Default::default() };
            let setup = prepare(d.to_str().unwrap(), &opts).unwrap();
            let tree = Arc::new(RwLock::new(new_tree(&setup)));
            let out = run(&setup, tree.clone(), ScanControl::new(), Progress::new());
            assert!(out.cancelled);
            assert!(out.errors[0].message.contains("root"), "{:?}", out.errors);
        }

        #[test]
        fn mft_letter_detection() {
            let mut s = prepare("C:\\", &ScanOptions::default()).unwrap();
            if s.volume.as_ref().is_some_and(|v| v.filesystem == "NTFS") {
                assert_eq!(s.mft_letter(), Some('C'));
            }
            s.display = "C:\\Users".into();
            assert_eq!(s.mft_letter(), None);
        }

        #[test]
        fn rejects_files_and_missing_paths() {
            let f = std::env::temp_dir().join(format!("treedock-file-{}", std::process::id()));
            std::fs::write(&f, b"x").unwrap();
            assert!(prepare(f.to_str().unwrap(), &ScanOptions::default()).is_err());
            std::fs::remove_file(&f).unwrap();
            assert!(prepare(r"C:\no\such\treedock\dir", &ScanOptions::default()).is_err());
        }
    }
}
