// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Scoped `SeBackupPrivilege` (MASTER_PLAN §8.1): enabled only for the
//! duration of an elevated scan and disabled again on drop. Never used for
//! file modifications.

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, LUID};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED, TOKEN_ADJUST_PRIVILEGES,
    TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use crate::util::wide_nul;

const ERROR_NOT_ALL_ASSIGNED: u32 = 1300;

pub struct BackupPrivilege {
    token: HANDLE,
    luid: LUID,
}

fn set(token: HANDLE, luid: LUID, enable: bool) -> bool {
    let tp = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: if enable { SE_PRIVILEGE_ENABLED } else { 0 } }],
    };
    let ok = unsafe { AdjustTokenPrivileges(token, 0, &tp, 0, std::ptr::null_mut(), std::ptr::null_mut()) };
    ok != 0 && unsafe { GetLastError() } != ERROR_NOT_ALL_ASSIGNED
}

impl BackupPrivilege {
    /// Enables the privilege; returns `None` if the token does not hold it.
    pub fn enable() -> Option<Self> {
        let mut token: HANDLE = std::ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token) } == 0 {
            return None;
        }
        let name = wide_nul("SeBackupPrivilege");
        let mut luid = LUID { LowPart: 0, HighPart: 0 };
        if unsafe { LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) } == 0 || !set(token, luid, true)
        {
            unsafe { CloseHandle(token) };
            return None;
        }
        Some(BackupPrivilege { token, luid })
    }
}

impl Drop for BackupPrivilege {
    fn drop(&mut self) {
        set(self.token, self.luid, false);
        unsafe { CloseHandle(self.token) };
    }
}
