// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Elevated NTFS fast path: reads `$MFT` directly from the volume
//! (MASTER_PLAN §5.2). Requires administrator rights; callers fall back to the
//! walk scanner on any error.

use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::sync::atomic::Ordering;

use treedock_core::scan::{Progress, ScanControl};
use treedock_core::tree::Tree;
use treedock_ntfs::{BuildOptions, BuildStats, MftIndex, record};
use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_BEGIN, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING, ReadFile,
    SetFilePointerEx,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::System::Ioctl::FSCTL_GET_NTFS_VOLUME_DATA;

use crate::util::wide_nul;

/// Read size for MFT extents.
const CHUNK: usize = 4 << 20;

#[derive(Debug, Clone, Default)]
pub struct NtfsVolumeData {
    pub serial: u64,
    pub bytes_per_sector: u32,
    pub bytes_per_cluster: u32,
    pub bytes_per_record: u32,
    pub mft_valid_data_length: u64,
    pub mft_start_lcn: u64,
}

#[derive(Debug, Clone, Default)]
pub struct MftScanStats {
    pub records_read: u64,
    pub records_in_use: u64,
    pub bad_records: u64,
    pub bytes_read: u64,
    pub build: BuildStats,
}

struct AlignedBuf {
    ptr: *mut u8,
    layout: Layout,
}

impl AlignedBuf {
    fn new(size: usize) -> Self {
        let layout = Layout::from_size_align(size, 4096).expect("layout");
        let ptr = unsafe { alloc_zeroed(layout) };
        assert!(!ptr.is_null(), "out of memory");
        AlignedBuf { ptr, layout }
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.layout.size()) }
    }
}

impl Drop for AlignedBuf {
    fn drop(&mut self) {
        unsafe { dealloc(self.ptr, self.layout) };
    }
}

struct Volume(HANDLE);

impl Drop for Volume {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

fn err(ctx: &str) -> String {
    format!("{ctx}: {}", std::io::Error::from_raw_os_error(unsafe { GetLastError() } as i32))
}

impl Volume {
    fn open(letter: char) -> Result<Self, String> {
        let p = wide_nul(&format!(r"\\.\{letter}:"));
        let h = unsafe {
            CreateFileW(
                p.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return Err(err("open volume (administrator rights are required)"));
        }
        Ok(Volume(h))
    }

    fn ntfs_data(&self) -> Result<NtfsVolumeData, String> {
        let mut b = [0u8; 128];
        let mut ret = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                self.0,
                FSCTL_GET_NTFS_VOLUME_DATA,
                std::ptr::null(),
                0,
                b.as_mut_ptr().cast(),
                b.len() as u32,
                &mut ret,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 || ret < 96 {
            return Err(err("FSCTL_GET_NTFS_VOLUME_DATA"));
        }
        let u32_at = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let u64_at = |o: usize| u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        let d = NtfsVolumeData {
            serial: u64_at(0),
            bytes_per_sector: u32_at(40),
            bytes_per_cluster: u32_at(44),
            bytes_per_record: u32_at(48),
            mft_valid_data_length: u64_at(56),
            mft_start_lcn: u64_at(64),
        };
        let pow2 = |v: u32| v >= 256 && v.is_power_of_two();
        if !pow2(d.bytes_per_sector)
            || !pow2(d.bytes_per_cluster)
            || !pow2(d.bytes_per_record)
            || d.bytes_per_record > 65536
        {
            return Err("unexpected NTFS geometry".into());
        }
        Ok(d)
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        if unsafe { SetFilePointerEx(self.0, offset as i64, std::ptr::null_mut(), FILE_BEGIN) } == 0 {
            return Err(err("seek"));
        }
        let mut done = 0u32;
        if unsafe { ReadFile(self.0, buf.as_mut_ptr(), buf.len() as u32, &mut done, std::ptr::null_mut()) } == 0 {
            return Err(err("read volume"));
        }
        Ok(done as usize)
    }
}

/// Reads the MFT of `letter:` and builds the tree. Progress counts records in `files`.
pub fn scan_volume(
    letter: char,
    root_display: &str,
    opts: &BuildOptions,
    control: &ScanControl,
    progress: &Progress,
) -> Result<(Tree, MftScanStats, NtfsVolumeData), String> {
    let vol = Volume::open(letter)?;
    let geo = vol.ntfs_data()?;
    let rec_size = geo.bytes_per_record as usize;
    let cluster = geo.bytes_per_cluster as u64;
    let sector = geo.bytes_per_sector as usize;

    // Record 0 describes $MFT itself; its unnamed $DATA runlist locates every extent.
    let mut first = AlignedBuf::new(rec_size.max(sector).max(4096));
    let n = vol.read_at(geo.mft_start_lcn * cluster, first.as_mut_slice())?;
    if n < rec_size {
        return Err("short read of MFT record 0".into());
    }
    let rec0 = &mut first.as_mut_slice()[..rec_size];
    record::apply_fixups(rec0, sector).map_err(|e| format!("MFT record 0: {e}"))?;
    let runs =
        record::data_runlist(rec0).map_err(|e| format!("MFT record 0 runlist: {e} (MFT may be too fragmented)"))?;

    let total_records = geo.mft_valid_data_length / rec_size as u64;
    let mut idx = MftIndex::with_capacity(total_records.min(u32::MAX as u64) as usize);
    let mut stats = MftScanStats::default();
    let mut buf = AlignedBuf::new(CHUNK);
    let mut record_no: u64 = 0;
    'runs: for run in runs {
        let run_bytes = run.clusters * cluster;
        let Some(lcn) = run.lcn else {
            // Sparse runs in $MFT should not occur; skip their record numbers.
            record_no += run_bytes / rec_size as u64;
            continue;
        };
        let mut done: u64 = 0;
        while done < run_bytes {
            if control.is_cancelled() {
                return Err("cancelled".into());
            }
            let want = (run_bytes - done).min(CHUNK as u64) as usize;
            let slice = &mut buf.as_mut_slice()[..want];
            let got = vol.read_at(lcn as u64 * cluster + done, slice)?;
            if got == 0 {
                return Err("unexpected end of volume".into());
            }
            stats.bytes_read += got as u64;
            for rec in slice[..got - got % rec_size].chunks_exact_mut(rec_size) {
                if record_no >= total_records {
                    break 'runs;
                }
                let _ = idx.ingest_raw(record_no, rec, sector);
                record_no += 1;
            }
            stats.records_read = record_no;
            progress.files.store(record_no, Ordering::Relaxed);
            done += got as u64;
        }
    }
    stats.records_in_use = idx.records_in_use;
    stats.bad_records = idx.bad_records;
    if stats.records_in_use == 0 {
        return Err("no MFT records could be read".into());
    }
    let (tree, build) = idx.build_tree(root_display, opts);
    stats.build = build;
    Ok((tree, stats, geo))
}
