//! The folders Windows names (Documents, Desktop, Downloads, the profile) and the drives, for the save dialog. The Documents
//! folder is asked for, not guessed from the profile: it is somewhere else when it is kept in OneDrive or moved.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::{FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Profile, SHGetKnownFolderPath};
use windows_sys::core::GUID;

use crate::browse::Place;

/// Where Windows keeps the folder `id`, if it can say.
fn known_folder(id: &GUID) -> Option<PathBuf> {
    let mut raw: *mut u16 = std::ptr::null_mut();
    // SAFETY: `id` is a valid folder id, no flags and no user token are passed, and `raw` is a valid out-pointer.
    let result = unsafe { SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut raw) };
    let path = if result >= 0 && !raw.is_null() {
        let mut len = 0;
        // SAFETY: on success `raw` is a NUL-ended wide string owned by us until it is freed below, read only up to its end.
        let units = unsafe {
            while *raw.add(len) != 0 {
                len += 1;
            }
            std::slice::from_raw_parts(raw, len)
        };
        Some(PathBuf::from(OsString::from_wide(units)))
    } else {
        None
    };
    // SAFETY: the string is ours to free with this call, on failure too (it may have been set).
    unsafe { CoTaskMemFree(raw.cast()) };
    path
}

/// Where a new document is first offered a place to be saved: the Documents folder, wherever it is kept (else the
/// profile, else the system drive).
pub fn documents_dir() -> PathBuf {
    [&FOLDERID_Documents, &FOLDERID_Profile].into_iter().filter_map(known_folder).find(|p| p.is_dir()).unwrap_or_else(|| {
        std::env::var_os("SystemDrive")
            .map_or_else(|| PathBuf::from("C:\\"), |d| PathBuf::from(format!("{}\\", d.to_string_lossy().trim_end_matches('\\'))))
    })
}

/// The places the save dialog offers in one press: the user's folders, then the drives (from the bit mask Windows keeps,
/// so that a disconnected network drive is not waited for).
pub fn places() -> Vec<Place> {
    let mut places = Vec::new();
    for (label, id) in
        [("Documents", &FOLDERID_Documents), ("Desktop", &FOLDERID_Desktop), ("Downloads", &FOLDERID_Downloads), ("Home", &FOLDERID_Profile)]
    {
        if let Some(path) = known_folder(id).filter(|p| p.is_dir()) {
            places.push(Place { label: label.to_string(), path });
        }
    }
    // SAFETY: takes no arguments.
    let drives = unsafe { GetLogicalDrives() };
    for (i, letter) in ('A'..='Z').enumerate() {
        if drives & (1 << i) != 0 {
            places.push(Place { label: format!("{letter}:"), path: PathBuf::from(format!("{letter}:\\")) });
        }
    }
    places
}
