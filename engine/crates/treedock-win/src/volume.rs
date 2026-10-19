// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Volume, media and privilege discovery (MASTER_PLAN §5.7, §8.1).

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, GetDiskFreeSpaceExW, GetDiskFreeSpaceW, GetDriveTypeW,
    GetFullPathNameW, GetLogicalDrives, GetVolumeInformationW, GetVolumePathNameW, OPEN_EXISTING,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::IOCTL_STORAGE_QUERY_PROPERTY;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use crate::util::{from_wide, wide_nul};

/// Storage class used to pick a concurrency profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Media {
    /// No seek penalty (SSD / NVMe).
    Ssd,
    /// Seek penalty (spinning disk).
    Hdd,
    Removable,
    Network,
    Unknown,
}

impl Media {
    pub fn as_str(&self) -> &'static str {
        match self {
            Media::Ssd => "ssd",
            Media::Hdd => "hdd",
            Media::Removable => "removable",
            Media::Network => "network",
            Media::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Option<Media> {
        Some(match s.to_ascii_lowercase().as_str() {
            "ssd" | "nvme" => Media::Ssd,
            "hdd" => Media::Hdd,
            "removable" | "usb" => Media::Removable,
            "network" | "smb" => Media::Network,
            _ => return None,
        })
    }

    /// Default number of walker threads (MASTER_PLAN §5.3; tune with benchmarks).
    pub fn default_threads(&self) -> usize {
        let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
        match self {
            Media::Ssd => (cores * 2).clamp(4, 32),
            Media::Hdd | Media::Unknown => 2,
            Media::Removable => 4,
            Media::Network => 24,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct VolumeInfo {
    /// Volume root as returned by `GetVolumePathNameW` (e.g. `C:\`, `\\srv\share\`).
    pub root: String,
    pub label: String,
    pub filesystem: String,
    pub serial: u32,
    pub fs_flags: u32,
    pub cluster_size: u32,
    pub total: u64,
    pub free: u64,
    pub drive_type: u32,
    pub media: Option<Media>,
}

const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;
const DRIVE_REMOTE: u32 = 4;
const DRIVE_CDROM: u32 = 5;

pub fn drive_type_name(t: u32) -> &'static str {
    match t {
        DRIVE_REMOVABLE => "removable",
        DRIVE_FIXED => "fixed",
        DRIVE_REMOTE => "network",
        DRIVE_CDROM => "optical",
        6 => "ramdisk",
        _ => "unknown",
    }
}

pub fn last_error_string() -> String {
    std::io::Error::from_raw_os_error(unsafe { GetLastError() } as i32).to_string()
}

/// Resolves a user-supplied path to a fully qualified path.
pub fn full_path(path: &str) -> Result<Vec<u16>, String> {
    let p = wide_nul(path);
    let mut buf = vec![0u16; 1024];
    loop {
        let n = unsafe { GetFullPathNameW(p.as_ptr(), buf.len() as u32, buf.as_mut_ptr(), std::ptr::null_mut()) };
        if n == 0 {
            return Err(last_error_string());
        }
        if (n as usize) < buf.len() {
            buf.truncate(n as usize);
            return Ok(buf);
        }
        buf.resize(n as usize + 1, 0);
    }
}

/// Mount root of the volume containing `full` (a fully qualified path).
pub fn volume_root(full: &[u16]) -> Result<String, String> {
    let p = crate::util::with_nul(full);
    let mut buf = vec![0u16; full.len() + 64];
    if unsafe { GetVolumePathNameW(p.as_ptr(), buf.as_mut_ptr(), buf.len() as u32) } == 0 {
        return Err(last_error_string());
    }
    Ok(from_wide(&buf))
}

pub fn volume_info(root: &str) -> Result<VolumeInfo, String> {
    let r = wide_nul(root);
    let mut label = [0u16; 261];
    let mut fs = [0u16; 261];
    let (mut serial, mut maxlen, mut fsflags) = (0u32, 0u32, 0u32);
    let ok = unsafe {
        GetVolumeInformationW(
            r.as_ptr(),
            label.as_mut_ptr(),
            label.len() as u32,
            &mut serial,
            &mut maxlen,
            &mut fsflags,
            fs.as_mut_ptr(),
            fs.len() as u32,
        )
    };
    if ok == 0 {
        return Err(last_error_string());
    }
    let mut info = VolumeInfo {
        root: root.to_owned(),
        label: from_wide(&label),
        filesystem: from_wide(&fs),
        serial,
        fs_flags: fsflags,
        drive_type: unsafe { GetDriveTypeW(r.as_ptr()) },
        ..Default::default()
    };
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    if unsafe { GetDiskFreeSpaceExW(r.as_ptr(), &mut avail, &mut total, &mut free) } != 0 {
        info.total = total;
        info.free = free;
    }
    let (mut spc, mut bps, mut fc, mut tc) = (0u32, 0u32, 0u32, 0u32);
    if unsafe { GetDiskFreeSpaceW(r.as_ptr(), &mut spc, &mut bps, &mut fc, &mut tc) } != 0 {
        info.cluster_size = spc.saturating_mul(bps);
    }
    info.media = Some(detect_media(&info));
    Ok(info)
}

fn detect_media(info: &VolumeInfo) -> Media {
    match info.drive_type {
        DRIVE_REMOTE => Media::Network,
        DRIVE_REMOVABLE => Media::Removable,
        DRIVE_FIXED => match seek_penalty(&info.root) {
            Some(false) => Media::Ssd,
            Some(true) => Media::Hdd,
            None => Media::Unknown,
        },
        _ => Media::Unknown,
    }
}

#[repr(C)]
struct StoragePropertyQuery {
    property_id: u32,
    query_type: u32,
    additional: [u8; 4],
}

#[repr(C)]
#[derive(Default)]
struct DeviceSeekPenaltyDescriptor {
    version: u32,
    size: u32,
    incurs_seek_penalty: u8,
}

const STORAGE_DEVICE_SEEK_PENALTY_PROPERTY: u32 = 7;

/// Queries `DEVICE_SEEK_PENALTY_DESCRIPTOR` for a drive-letter volume.
/// Opens the volume with access mask 0, which does not require elevation.
pub fn seek_penalty(root: &str) -> Option<bool> {
    let letter = root.chars().next().filter(|c| c.is_ascii_alphabetic() && root[1..].starts_with(':'))?;
    let dev = wide_nul(&format!(r"\\.\{letter}:"));
    let h: HANDLE = unsafe {
        CreateFileW(
            dev.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    if h == INVALID_HANDLE_VALUE {
        return None;
    }
    let q =
        StoragePropertyQuery { property_id: STORAGE_DEVICE_SEEK_PENALTY_PROPERTY, query_type: 0, additional: [0; 4] };
    let mut d = DeviceSeekPenaltyDescriptor::default();
    let mut ret = 0u32;
    let ok = unsafe {
        DeviceIoControl(
            h,
            IOCTL_STORAGE_QUERY_PROPERTY,
            (&q as *const StoragePropertyQuery).cast(),
            std::mem::size_of::<StoragePropertyQuery>() as u32,
            (&mut d as *mut DeviceSeekPenaltyDescriptor).cast(),
            std::mem::size_of::<DeviceSeekPenaltyDescriptor>() as u32,
            &mut ret,
            std::ptr::null_mut(),
        )
    };
    unsafe { CloseHandle(h) };
    if ok == 0 || (ret as usize) < 9 {
        return None;
    }
    Some(d.incurs_seek_penalty != 0)
}

/// Drive-letter volumes that are ready. Volumes that fail to report are skipped.
pub fn list_volumes() -> Vec<VolumeInfo> {
    let mask = unsafe { GetLogicalDrives() };
    (0..26u32)
        .filter(|i| mask & (1 << i) != 0)
        .filter_map(|i| volume_info(&format!("{}:\\", (b'A' + i as u8) as char)).ok())
        .collect()
}

/// True when the current process token is elevated.
pub fn is_elevated() -> bool {
    let mut token: HANDLE = std::ptr::null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return false;
    }
    let mut e = TOKEN_ELEVATION { TokenIsElevated: 0 };
    let mut ret = 0u32;
    let ok = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            (&mut e as *mut TOKEN_ELEVATION).cast(),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret,
        )
    };
    unsafe { CloseHandle(token) };
    ok != 0 && e.TokenIsElevated != 0
}

/// Peak working set of the current process, in bytes.
pub fn peak_working_set() -> u64 {
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    let mut c: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
    c.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
    if unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, c.cb) } == 0 {
        return 0;
    }
    c.PeakWorkingSetSize as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_volume_info() {
        let tmp = std::env::temp_dir();
        let full = full_path(tmp.to_str().unwrap()).unwrap();
        let root = volume_root(&full).unwrap();
        assert!(root.ends_with('\\'), "{root}");
        let v = volume_info(&root).unwrap();
        assert!(!v.filesystem.is_empty());
        assert!(v.cluster_size >= 512);
        assert!(v.total > 0 && v.total >= v.free);
        assert!(!list_volumes().is_empty());
        let _ = is_elevated();
    }
}
