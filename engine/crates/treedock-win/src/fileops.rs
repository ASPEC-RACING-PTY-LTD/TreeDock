// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Safe file operations (MASTER_PLAN §8.2).
//!
//! * Recycle Bin is the only deletion offered here; it never silently
//!   falls back to permanent deletion (`FOF_WANTNUKEWARNING`).
//! * Every item is re-validated against the scan (file ID, size, last-write
//!   time) immediately before acting; changed items are skipped.
//! * Protected system locations are refused.
//! * Items are processed one at a time so a bulk operation can be cancelled
//!   between items and reports a per-item result.

use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::UI::Shell::{
    FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FOF_WANTNUKEWARNING, FOFX_ADDUNDORECORD, FOFX_EARLYFAILURE,
    FOFX_RECYCLEONDELETE, FileOperation, IFileOperation, IShellItem, SHCreateItemFromParsingName,
};
use windows::core::PCWSTR;
use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_NO_RECALL,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    GetFileAttributesW, GetFileInformationByHandle, INVALID_FILE_ATTRIBUTES, OPEN_EXISTING,
};

use crate::util;

/// What the scan recorded about an item, used for re-validation.
#[derive(Clone, Debug)]
pub struct Expected {
    pub file_id: u64,
    pub logical: u64,
    pub mtime: i64,
    pub is_dir: bool,
}

#[derive(Clone, Debug)]
pub struct OpItem {
    /// Normal (display) path.
    pub path: String,
    pub expected: Expected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpStatus {
    Done,
    Skipped,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct OpResult {
    pub path: String,
    pub status: OpStatus,
    pub message: String,
}

/// Locations TreeDock refuses to delete, with the reason shown to the user.
pub fn protected_reason(path: &str) -> Option<String> {
    let p = path.trim_end_matches('\\').to_ascii_lowercase();
    let env = |k: &str| std::env::var(k).ok().map(|v| v.trim_end_matches('\\').to_ascii_lowercase());
    if p.len() <= 2 || (p.len() == 3 && p.ends_with(":\\")) || p.matches('\\').count() == 0 {
        return Some("Volume roots cannot be deleted.".into());
    }
    let name = p.rsplit('\\').next().unwrap_or("");
    if matches!(name, "pagefile.sys" | "hiberfil.sys" | "swapfile.sys" | "dumpstack.log.tmp") {
        return Some("This is a Windows system file. Manage it from System settings (virtual memory / power).".into());
    }
    if name == "system volume information" || name == "$recycle.bin" {
        return Some("This folder is managed by Windows (restore points, Recycle Bin).".into());
    }
    let under = |base: Option<String>| base.is_some_and(|b| p == b || p.starts_with(&format!("{b}\\")));
    if under(env("SystemRoot")) {
        return Some("Files under the Windows folder are protected. Use Disk Cleanup or Storage Sense instead.".into());
    }
    for k in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        if env(k).is_some_and(|b| p == b) {
            return Some("Program Files cannot be deleted. Uninstall apps from Settings instead.".into());
        }
    }
    if under(env("ProgramData").map(|d| format!("{d}\\microsoft"))) {
        return Some("Windows and Microsoft app data is protected.".into());
    }
    if env("USERPROFILE").is_some_and(|u| p == u) || env("SystemDrive").is_some_and(|d| p == format!("{d}\\users")) {
        return Some("User profile folders cannot be deleted as a whole.".into());
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
        && p.starts_with(&dir.to_string_lossy().to_ascii_lowercase())
    {
        return Some("TreeDock's own files cannot be deleted while it is running.".into());
    }
    None
}

/// Checks that the item on disk is still the one that was scanned.
/// Never opens file data and never triggers cloud recall.
pub fn revalidate(item: &OpItem) -> Result<(), String> {
    let ext = util::with_nul(&util::to_extended(&util::wide(&item.path)));
    let attrs = unsafe { GetFileAttributesW(ext.as_ptr()) };
    if attrs == INVALID_FILE_ATTRIBUTES {
        return Err("no longer exists".into());
    }
    let is_dir = attrs & 0x10 != 0;
    if is_dir != item.expected.is_dir {
        return Err("changed type since the scan".into());
    }
    let h = unsafe {
        CreateFileW(
            ext.as_ptr(),
            FILE_READ_ATTRIBUTES,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_OPEN_NO_RECALL,
            std::ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(crate::volume::last_error_string());
    }
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetFileInformationByHandle(h, &mut info) };
    unsafe { CloseHandle(h) };
    if ok == 0 {
        return Err(crate::volume::last_error_string());
    }
    let id = ((info.nFileIndexHigh as u64) << 32) | info.nFileIndexLow as u64;
    let size = ((info.nFileSizeHigh as u64) << 32) | info.nFileSizeLow as u64;
    let mtime = ((info.ftLastWriteTime.dwHighDateTime as i64) << 32) | info.ftLastWriteTime.dwLowDateTime as i64;
    // The walk scanner may record 128-bit IDs truncated to 64 bits; compare the low 64 bits when known.
    if item.expected.file_id != 0
        && id != item.expected.file_id
        && (id & 0x0000_FFFF_FFFF_FFFF) != (item.expected.file_id & 0x0000_FFFF_FFFF_FFFF)
    {
        return Err("was replaced by a different file since the scan".into());
    }
    if !is_dir && (size != item.expected.logical || mtime != item.expected.mtime) {
        return Err("was modified since the scan".into());
    }
    Ok(())
}

struct ComGuard;

impl ComGuard {
    fn new() -> Self {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
        }
        ComGuard
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn recycle_one(path: &str, owner: isize) -> windows::core::Result<bool> {
    unsafe {
        let op: IFileOperation = CoCreateInstance(&FileOperation, None, CLSCTX_ALL)?;
        // Recycle only; warn (and let the user decline) if an item would be destroyed instead.
        op.SetOperationFlags(
            FOF_NOCONFIRMATION
                | FOF_SILENT
                | FOF_NOERRORUI
                | FOF_WANTNUKEWARNING
                | FOFX_RECYCLEONDELETE
                | FOFX_ADDUNDORECORD
                | FOFX_EARLYFAILURE,
        )?;
        if owner != 0 {
            op.SetOwnerWindow(HWND(owner as *mut _))?;
        }
        let w = util::wide_nul(path);
        let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None)?;
        op.DeleteItem(&item, None)?;
        op.PerformOperations()?;
        Ok(!op.GetAnyOperationsAborted()?.as_bool())
    }
}

/// Sends items to the Recycle Bin, one by one. `owner` is an HWND (0 = none)
/// used to parent any shell prompt. Runs COM on the calling thread (call it
/// from a worker thread, not the UI thread).
pub fn recycle(
    items: &[OpItem],
    owner: isize,
    cancel: &AtomicBool,
    mut on_item: impl FnMut(usize, &OpResult),
) -> Vec<OpResult> {
    let _com = ComGuard::new();
    let mut results = Vec::with_capacity(items.len());
    for (i, item) in items.iter().enumerate() {
        let r = if cancel.load(Ordering::Relaxed) {
            OpResult { path: item.path.clone(), status: OpStatus::Cancelled, message: "cancelled".into() }
        } else if let Some(reason) = protected_reason(&item.path) {
            OpResult { path: item.path.clone(), status: OpStatus::Skipped, message: reason }
        } else if let Err(why) = revalidate(item) {
            OpResult { path: item.path.clone(), status: OpStatus::Skipped, message: format!("Skipped: {why}.") }
        } else {
            match recycle_one(&item.path, owner) {
                Ok(true) if !exists(&item.path) => OpResult {
                    path: item.path.clone(),
                    status: OpStatus::Done,
                    message: "Moved to Recycle Bin.".into(),
                },
                Ok(_) => OpResult {
                    path: item.path.clone(),
                    status: OpStatus::Failed,
                    message: "Not recycled (declined, or the item is in use).".into(),
                },
                Err(e) => {
                    OpResult { path: item.path.clone(), status: OpStatus::Failed, message: e.message().to_string() }
                }
            }
        };
        on_item(i, &r);
        results.push(r);
    }
    results
}

fn exists(path: &str) -> bool {
    let ext = util::with_nul(&util::to_extended(&util::wide(path)));
    unsafe { GetFileAttributesW(ext.as_ptr()) != INVALID_FILE_ATTRIBUTES }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_locations() {
        assert!(protected_reason("C:\\").is_some());
        assert!(protected_reason("C:\\pagefile.sys").is_some());
        assert!(protected_reason("D:\\System Volume Information").is_some());
        if let Ok(w) = std::env::var("SystemRoot") {
            assert!(protected_reason(&format!("{w}\\System32\\drivers")).is_some());
        }
        if let Ok(u) = std::env::var("USERPROFILE") {
            assert!(protected_reason(&u).is_some());
            assert!(protected_reason(&format!("{u}\\Downloads\\old.iso")).is_none());
        }
    }

    #[test]
    fn revalidation_detects_changes() {
        let d = std::env::temp_dir().join(format!("treedock-ops-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("x.bin");
        std::fs::write(&f, vec![1u8; 100]).unwrap();
        let path = f.to_str().unwrap().to_string();
        let mut item =
            OpItem { path: path.clone(), expected: Expected { file_id: 0, logical: 100, mtime: 0, is_dir: false } };
        // Wrong mtime → modified.
        assert!(revalidate(&item).unwrap_err().contains("modified"));
        // Take real values from a fresh enumeration.
        let e = crate::enumerate::WinEnumerator::new(false);
        let mut out = Vec::new();
        treedock_core::scan::DirEnumerator::enumerate(
            &e,
            &util::to_extended(&util::wide(d.to_str().unwrap())),
            &mut out,
        )
        .unwrap();
        let rec = out.iter().find(|r| String::from_utf16_lossy(&r.name) == "x.bin").unwrap();
        item.expected = Expected { file_id: rec.file_id, logical: rec.logical, mtime: rec.mtime, is_dir: false };
        revalidate(&item).unwrap();
        std::fs::write(&f, vec![1u8; 101]).unwrap();
        assert!(revalidate(&item).is_err());
        std::fs::remove_file(&f).unwrap();
        assert!(revalidate(&item).unwrap_err().contains("no longer exists"));
        std::fs::remove_dir_all(&d).unwrap();
    }
}

#[cfg(test)]
mod manual_tests {
    use super::*;

    /// Puts a temporary file into the real Recycle Bin. Run explicitly:
    /// `cargo test -p treedock-win recycle_temp_file -- --ignored`
    #[test]
    #[ignore]
    fn recycle_temp_file() {
        let d = std::env::temp_dir().join(format!("treedock-recycle-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("treedock-recycle-test.txt");
        std::fs::write(&f, b"TreeDock recycle test file - safe to delete").unwrap();
        let e = crate::enumerate::WinEnumerator::new(false);
        let mut out = Vec::new();
        treedock_core::scan::DirEnumerator::enumerate(
            &e,
            &util::to_extended(&util::wide(d.to_str().unwrap())),
            &mut out,
        )
        .unwrap();
        let rec = out.iter().find(|r| String::from_utf16_lossy(&r.name) == "treedock-recycle-test.txt").unwrap();
        let item = OpItem {
            path: f.to_str().unwrap().into(),
            expected: Expected { file_id: rec.file_id, logical: rec.logical, mtime: rec.mtime, is_dir: false },
        };
        let stale = OpItem { expected: Expected { mtime: 1, ..item.expected.clone() }, ..item.clone() };
        let res = recycle(&[stale, item], 0, &AtomicBool::new(false), |_, _| {});
        assert_eq!(res[0].status, OpStatus::Skipped, "{:?}", res[0]);
        assert_eq!(res[1].status, OpStatus::Done, "{:?}", res[1]);
        assert!(!f.exists());
        std::fs::remove_dir(&d).unwrap();
    }
}
