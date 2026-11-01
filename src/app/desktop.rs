//! Desktop folder discovery + input/output folder setup (Windows).

use std::path::PathBuf;
use windows_sys::Win32::Foundation::S_OK;
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::{SHGetKnownFolderPath, FOLDERID_Desktop};

/// Resolve the current user's Desktop folder (OneDrive-redirected desktops included).
pub fn desktop_dir() -> PathBuf {
    if let Some(p) = known_desktop() {
        return p;
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        return PathBuf::from(profile).join("Desktop");
    }
    PathBuf::from(".")
}

fn known_desktop() -> Option<PathBuf> {
    unsafe {
        let mut path_ptr: *mut u16 = std::ptr::null_mut();
        let hr = SHGetKnownFolderPath(
            &FOLDERID_Desktop,
            0,
            std::ptr::null_mut(),
            &mut path_ptr,
        );
        if hr != S_OK || path_ptr.is_null() {
            return None;
        }
        let mut len = 0usize;
        while *path_ptr.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(path_ptr, len);
        let s = String::from_utf16_lossy(slice);
        CoTaskMemFree(path_ptr as *const _);
        Some(PathBuf::from(s))
    }
}
