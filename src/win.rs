//! The Win32 calls clippath needs: the clipboard and two known folders.

use std::io;
use std::path::PathBuf;
use std::ptr::null_mut;
use std::thread::sleep;
use std::time::Duration;

use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::Foundation::GlobalFree;
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows_sys::Win32::System::Ole::{CF_DIB, CF_UNICODETEXT};
use windows_sys::Win32::UI::Shell::{FOLDERID_Screenshots, SHGetKnownFolderPath};

pub const DIB: u32 = CF_DIB as u32;
pub const UNICODE_TEXT: u32 = CF_UNICODETEXT as u32;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

pub fn register_format(name: &str) -> u32 {
    unsafe { RegisterClipboardFormatW(wide(name).as_ptr()) }
}

/// Changes on every clipboard update; used to notice a copy made in the meantime.
pub fn sequence() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

/// The open clipboard; closed when dropped.
pub struct Clipboard(());

impl Clipboard {
    pub fn open() -> io::Result<Self> {
        for _ in 0..50 {
            // Another app may hold the clipboard for a moment.
            if unsafe { OpenClipboard(null_mut()) } != 0 {
                return Ok(Clipboard(()));
            }
            sleep(Duration::from_millis(20));
        }
        Err(io::Error::other("clipboard busy"))
    }

    pub fn get(&self, format: u32) -> Option<Vec<u8>> {
        unsafe {
            let handle = GetClipboardData(format);
            if handle.is_null() {
                return None;
            }
            let ptr = GlobalLock(handle) as *const u8;
            if ptr.is_null() {
                return None;
            }
            let data = std::slice::from_raw_parts(ptr, GlobalSize(handle)).to_vec();
            GlobalUnlock(handle);
            Some(data)
        }
    }

    /// Take ownership, which SetClipboardData requires; drops what was there.
    pub fn empty(&self) -> io::Result<()> {
        if unsafe { EmptyClipboard() } == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
    }

    pub fn put(&self, format: u32, data: &[u8]) -> io::Result<()> {
        unsafe {
            let handle = GlobalAlloc(GMEM_MOVEABLE, data.len());
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), GlobalLock(handle) as *mut u8, data.len());
            GlobalUnlock(handle);
            // On success the clipboard owns the memory; on failure it is still ours.
            if SetClipboardData(format, handle).is_null() {
                let err = io::Error::last_os_error();
                GlobalFree(handle);
                return Err(err);
            }
            Ok(())
        }
    }

    pub fn put_text(&self, text: &str) -> io::Result<()> {
        let bytes: Vec<u8> = wide(text).iter().flat_map(|c| c.to_le_bytes()).collect();
        self.put(UNICODE_TEXT, &bytes)
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        unsafe { CloseClipboard() };
    }
}

/// Where Snipping Tool and Win+PrtScn save screenshots.
pub fn screenshots_dir() -> Option<PathBuf> {
    unsafe {
        let mut path = null_mut();
        if SHGetKnownFolderPath(&FOLDERID_Screenshots, 0, null_mut(), &mut path) != 0 {
            return None;
        }
        let len = (0..).take_while(|&i| *path.add(i) != 0).count();
        let result = String::from_utf16(std::slice::from_raw_parts(path, len)).ok().map(PathBuf::from);
        CoTaskMemFree(path as _);
        result
    }
}

/// %LOCALAPPDATA%\clippath, where clipboard images that no tool saved go.
pub fn folder() -> io::Result<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").ok_or_else(|| io::Error::other("LOCALAPPDATA is not set"))?;
    let dir = PathBuf::from(base).join("clippath");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
