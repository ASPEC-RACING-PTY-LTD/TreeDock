// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Universal directory enumerator (MASTER_PLAN §5.3).
//!
//! Each directory is opened once and read in bulk with
//! `GetFileInformationByHandleEx`, which returns sizes, *allocation* sizes,
//! attributes, times, file IDs and (for the extended class) reparse tags
//! without opening individual files. Fallback chain:
//! `FileIdExtdDirectoryInfo` → `FileIdBothDirectoryInfo` → `FindFirstFileExW`
//! (no allocation size, so it is estimated and flagged).

use std::cell::RefCell;
use std::sync::atomic::{AtomicU8, Ordering};

use treedock_core::scan::{DirEnumerator, EnumError, RawEntry};
use treedock_core::tree::attr;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_INVALID_FUNCTION, ERROR_INVALID_LEVEL, ERROR_INVALID_PARAMETER, ERROR_NO_MORE_FILES,
    ERROR_NOT_SUPPORTED, GetLastError, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_LIST_DIRECTORY, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    FIND_FIRST_EX_LARGE_FETCH, FileIdBothDirectoryInfo, FileIdExtdDirectoryInfo, FindClose, FindExInfoBasic,
    FindExSearchNameMatch, FindFirstFileExW, FindNextFileW, GetFileInformationByHandleEx, OPEN_EXISTING, SYNCHRONIZE,
    WIN32_FIND_DATAW,
};
use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN};

use crate::util;

const MODE_EXTD: u8 = 0;
const MODE_BOTH: u8 = 1;
const MODE_FIND: u8 = 2;

/// 64 KiB, 8-byte aligned.
const BUF_WORDS: usize = 64 * 1024 / 8;

thread_local! {
    static BUF: RefCell<Vec<u64>> = RefCell::new(vec![0u64; BUF_WORDS]);
}

pub struct WinEnumerator {
    mode: AtomicU8,
    low_impact: bool,
}

impl WinEnumerator {
    pub fn new(low_impact: bool) -> Self {
        WinEnumerator { mode: AtomicU8::new(MODE_EXTD), low_impact }
    }

    /// Name of the information class currently in use (for diagnostics).
    pub fn mode_name(&self) -> &'static str {
        match self.mode.load(Ordering::Relaxed) {
            MODE_EXTD => "FileIdExtdDirectoryInfo",
            MODE_BOTH => "FileIdBothDirectoryInfo",
            _ => "FindFirstFileExW",
        }
    }

    /// True when sizes on disk are estimated (FindFirstFileExW fallback).
    pub fn allocation_estimated(&self) -> bool {
        self.mode.load(Ordering::Relaxed) == MODE_FIND
    }
}

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

pub fn os_error(code: u32) -> EnumError {
    EnumError { code, message: std::io::Error::from_raw_os_error(code as i32).to_string() }
}

#[inline]
fn rd_u32(b: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(b[off..off + 4].try_into().unwrap())
}

#[inline]
fn rd_i64(b: &[u8], off: usize) -> i64 {
    i64::from_le_bytes(b[off..off + 8].try_into().unwrap())
}

fn name_at(b: &[u8], off: usize, bytes: usize) -> Vec<u16> {
    b[off..off + bytes].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect()
}

/// Parses a buffer of `FILE_ID_EXTD_DIR_INFO` or `FILE_ID_BOTH_DIR_INFO` records.
fn parse_records(b: &[u8], extd: bool, out: &mut Vec<RawEntry>) {
    let header = if extd { 88 } else { 104 };
    let mut off = 0usize;
    loop {
        if off + header > b.len() {
            break;
        }
        let next = rd_u32(b, off) as usize;
        let name_bytes = rd_u32(b, off + 60) as usize;
        let attrs = rd_u32(b, off + 56);
        let (name_off, file_id, tag) = if extd {
            // FileId is a 16-byte FILE_ID_128; keep the low 64 bits (NTFS FRN fits).
            (off + 88, u64::from_le_bytes(b[off + 72..off + 80].try_into().unwrap()), rd_u32(b, off + 68))
        } else {
            // For FILE_ID_BOTH_DIR_INFO, EaSize carries the reparse tag for reparse points.
            (off + 104, rd_i64(b, off + 96) as u64, rd_u32(b, off + 64))
        };
        if name_off + name_bytes > b.len() {
            break;
        }
        out.push(RawEntry {
            name: name_at(b, name_off, name_bytes),
            attrs,
            logical: rd_i64(b, off + 40).max(0) as u64,
            allocated: Some(rd_i64(b, off + 48).max(0) as u64),
            mtime: rd_i64(b, off + 24),
            ctime: rd_i64(b, off + 8),
            file_id,
            reparse_tag: if attrs & attr::REPARSE_POINT != 0 { tag } else { 0 },
        });
        if next == 0 {
            break;
        }
        off += next;
    }
}

enum ByHandle {
    Done,
    Unsupported,
}

fn enumerate_by_handle(path: &[u16], extd: bool, out: &mut Vec<RawEntry>) -> Result<ByHandle, EnumError> {
    let p = util::with_nul(path);
    let h = unsafe {
        CreateFileW(
            p.as_ptr(),
            FILE_LIST_DIRECTORY | SYNCHRONIZE,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(os_error(unsafe { GetLastError() }));
    }
    let h = Handle(h);
    let class = if extd { FileIdExtdDirectoryInfo } else { FileIdBothDirectoryInfo };
    BUF.with(|buf| {
        let mut buf = buf.borrow_mut();
        let mut first = true;
        loop {
            let ok =
                unsafe { GetFileInformationByHandleEx(h.0, class, buf.as_mut_ptr().cast(), (buf.len() * 8) as u32) };
            if ok == 0 {
                let e = unsafe { GetLastError() };
                if e == ERROR_NO_MORE_FILES {
                    return Ok(ByHandle::Done);
                }
                if first
                    && matches!(
                        e,
                        ERROR_INVALID_PARAMETER | ERROR_INVALID_FUNCTION | ERROR_NOT_SUPPORTED | ERROR_INVALID_LEVEL
                    )
                {
                    return Ok(ByHandle::Unsupported);
                }
                return Err(os_error(e));
            }
            first = false;
            let bytes = unsafe { std::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), buf.len() * 8) };
            parse_records(bytes, extd, out);
        }
    })
}

fn enumerate_find(path: &[u16], out: &mut Vec<RawEntry>) -> Result<(), EnumError> {
    let mut pattern = util::join(path, &util::wide("*"));
    pattern.push(0);
    let mut fd: WIN32_FIND_DATAW = unsafe { std::mem::zeroed() };
    let h = unsafe {
        FindFirstFileExW(
            pattern.as_ptr(),
            FindExInfoBasic,
            (&mut fd as *mut WIN32_FIND_DATAW).cast(),
            FindExSearchNameMatch,
            std::ptr::null(),
            FIND_FIRST_EX_LARGE_FETCH,
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return Err(os_error(unsafe { GetLastError() }));
    }
    let ft = |f: windows_sys::Win32::Foundation::FILETIME| ((f.dwHighDateTime as i64) << 32) | f.dwLowDateTime as i64;
    loop {
        let name_len = fd.cFileName.iter().position(|&c| c == 0).unwrap_or(fd.cFileName.len());
        out.push(RawEntry {
            name: fd.cFileName[..name_len].to_vec(),
            attrs: fd.dwFileAttributes,
            logical: ((fd.nFileSizeHigh as u64) << 32) | fd.nFileSizeLow as u64,
            allocated: None,
            mtime: ft(fd.ftLastWriteTime),
            ctime: ft(fd.ftCreationTime),
            file_id: 0,
            reparse_tag: if fd.dwFileAttributes & attr::REPARSE_POINT != 0 { fd.dwReserved0 } else { 0 },
        });
        if unsafe { FindNextFileW(h, &mut fd) } == 0 {
            let e = unsafe { GetLastError() };
            unsafe { FindClose(h) };
            return if e == ERROR_NO_MORE_FILES { Ok(()) } else { Err(os_error(e)) };
        }
    }
}

impl DirEnumerator for WinEnumerator {
    /// Extended-length (`\\?\`) UTF-16 path without terminator.
    type Path = Vec<u16>;

    fn enumerate(&self, path: &Vec<u16>, out: &mut Vec<RawEntry>) -> Result<(), EnumError> {
        loop {
            let mode = self.mode.load(Ordering::Relaxed);
            let start = out.len();
            let r = match mode {
                MODE_EXTD | MODE_BOTH => enumerate_by_handle(path, mode == MODE_EXTD, out),
                _ => return enumerate_find(path, out),
            };
            match r {
                Ok(ByHandle::Done) => return Ok(()),
                Ok(ByHandle::Unsupported) => {
                    out.truncate(start);
                    let _ = self.mode.compare_exchange(mode, mode + 1, Ordering::Relaxed, Ordering::Relaxed);
                }
                Err(e) => return Err(e),
            }
        }
    }

    fn join(&self, parent: &Vec<u16>, name: &[u16]) -> Vec<u16> {
        util::join(parent, name)
    }

    fn display(&self, path: &Vec<u16>) -> String {
        util::to_display(path)
    }

    fn on_worker_start(&self) {
        if self.low_impact {
            // Lowers CPU and I/O scheduling priority for this thread.
            unsafe { SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("treedock-enum-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn ext_path(p: &std::path::Path) -> Vec<u16> {
        util::to_extended(&util::wide(p.to_str().unwrap()))
    }

    #[test]
    fn all_modes_agree() {
        let d = tempdir("modes");
        std::fs::write(d.join("a.txt"), vec![1u8; 5000]).unwrap();
        std::fs::create_dir(d.join("sub")).unwrap();
        std::fs::write(d.join("名前 🎉.bin"), b"x").unwrap();
        let mut results = Vec::new();
        for mode in [MODE_EXTD, MODE_BOTH, MODE_FIND] {
            let e = WinEnumerator::new(false);
            e.mode.store(mode, Ordering::Relaxed);
            let mut out = Vec::new();
            e.enumerate(&ext_path(&d), &mut out).unwrap();
            let mut v: Vec<(String, u64, bool)> = out
                .iter()
                .filter(|r| r.name != [0x2E] && r.name != [0x2E, 0x2E])
                .map(|r| (String::from_utf16_lossy(&r.name), r.logical, r.attrs & attr::DIRECTORY != 0))
                .collect();
            v.sort();
            if mode != MODE_FIND {
                let a = out.iter().find(|r| String::from_utf16_lossy(&r.name) == "a.txt").unwrap();
                assert!(a.allocated.unwrap() >= 5000);
                assert_ne!(a.file_id, 0);
                assert!(a.mtime > 0);
            }
            results.push(v);
        }
        assert_eq!(
            results[0],
            vec![("a.txt".into(), 5000, false), ("sub".into(), 0, true), ("名前 🎉.bin".into(), 1, false)]
        );
        assert_eq!(results[0], results[1]);
        assert_eq!(results[0], results[2]);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn missing_dir_is_error() {
        let e = WinEnumerator::new(false);
        let mut out = Vec::new();
        let err = e.enumerate(&util::wide(r"\\?\C:\definitely\not\here\treedock"), &mut out).unwrap_err();
        assert!(err.code == 2 || err.code == 3, "{err:?}");
    }
}
