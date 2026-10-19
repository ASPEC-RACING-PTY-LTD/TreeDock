// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
//! Effective-access check for the current user (MASTER_PLAN §8.1).
//!
//! Used to decide whether TreeDock should offer elevation before scanning
//! and managing a folder. "Full access" here means what file management
//! needs: the current (possibly UAC-filtered) token is granted read, write,
//! traverse and delete rights (the "Modify" permission), not ownership or
//! permission changes.

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{
    AccessCheck, DACL_SECURITY_INFORMATION, DuplicateToken, GENERIC_MAPPING, GROUP_SECURITY_INFORMATION,
    GetFileSecurityW, MapGenericMask, OWNER_SECURITY_INFORMATION, PRIVILEGE_SET, SecurityImpersonation,
    TOKEN_DUPLICATE, TOKEN_QUERY,
};
use windows_sys::Win32::Storage::FileSystem::{
    DELETE, FILE_ALL_ACCESS, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use crate::util;

struct Token(HANDLE);

impl Drop for Token {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { CloseHandle(self.0) };
        }
    }
}

/// Returns `Ok(true)` when the current user has full control of `path`.
pub fn has_full_access(path: &str) -> Result<bool, String> {
    let full = crate::volume::full_path(path)?;
    let p = util::with_nul(&util::to_extended(&full));
    let info = OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    let mut needed = 0u32;
    unsafe { GetFileSecurityW(p.as_ptr(), info, std::ptr::null_mut(), 0, &mut needed) };
    if needed == 0 {
        // Cannot even read the security descriptor: certainly not full access.
        return Ok(false);
    }
    let mut sd = vec![0u64; (needed as usize).div_ceil(8)];
    if unsafe { GetFileSecurityW(p.as_ptr(), info, sd.as_mut_ptr().cast(), needed, &mut needed) } == 0 {
        return Ok(false);
    }
    let mut process = Token(std::ptr::null_mut());
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY | TOKEN_DUPLICATE, &mut process.0) } == 0 {
        return Err(crate::volume::last_error_string());
    }
    let mut imp = Token(std::ptr::null_mut());
    if unsafe { DuplicateToken(process.0, SecurityImpersonation, &mut imp.0) } == 0 {
        return Err(crate::volume::last_error_string());
    }
    let mapping = GENERIC_MAPPING {
        GenericRead: FILE_GENERIC_READ,
        GenericWrite: FILE_GENERIC_WRITE,
        GenericExecute: FILE_GENERIC_EXECUTE,
        GenericAll: FILE_ALL_ACCESS,
    };
    let mut desired = FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE | DELETE;
    unsafe { MapGenericMask(&mut desired, &mapping) };
    let mut privs = [0u64; 32];
    let mut privs_len = std::mem::size_of_val(&privs) as u32;
    let (mut granted, mut status) = (0u32, 0i32);
    let ok = unsafe {
        AccessCheck(
            sd.as_mut_ptr().cast(),
            imp.0,
            desired,
            &mapping,
            privs.as_mut_ptr().cast::<PRIVILEGE_SET>(),
            &mut privs_len,
            &mut granted,
            &mut status,
        )
    };
    if ok == 0 {
        return Err(crate::volume::last_error_string());
    }
    Ok(status != 0 && granted & desired == desired)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_is_fully_accessible() {
        let d = std::env::temp_dir().join(format!("treedock-acl-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(has_full_access(d.to_str().unwrap()), Ok(true));
        std::fs::remove_dir(&d).unwrap();
    }

    #[test]
    fn system_folders_are_not_fully_accessible_without_elevation() {
        if crate::volume::is_elevated() {
            return;
        }
        let sys = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        assert_eq!(has_full_access(&format!(r"{sys}\System32")), Ok(false));
    }
}
