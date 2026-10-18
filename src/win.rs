//! The Win32 calls clippath needs: the clipboard, the paste target, a few known
//! folders, the single-instance mutex and the registry.

use std::io;
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::thread::sleep;
use std::time::Duration;

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, GlobalFree, HANDLE, HWND};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
    OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock};
use windows_sys::Win32::System::Ole::{CF_DIB, CF_UNICODETEXT};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, REG_DWORD, REG_SZ, RRF_RT_REG_DWORD, RRF_RT_REG_SZ, RegCloseKey,
    RegDeleteKeyValueW, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW, RegSetKeyValueW,
};
use windows_sys::Win32::System::Threading::{
    CreateMutexW, OpenMutexW, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::Shell::{FOLDERID_Screenshots, SHGetKnownFolderPath};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId};

pub const DIB: u32 = CF_DIB as u32;
pub const UNICODE_TEXT: u32 = CF_UNICODETEXT as u32;
const SYNCHRONIZE: u32 = 0x0010_0000;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

fn from_wide(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

pub fn register_format(name: &str) -> u32 {
    unsafe { RegisterClipboardFormatW(wide(name).as_ptr()) }
}

/// Changes on every clipboard update; used to notice a copy made in the meantime.
pub fn sequence() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

pub fn has_format(format: u32) -> bool {
    unsafe { IsClipboardFormatAvailable(format) != 0 }
}

/// The open clipboard; closed when dropped.
pub struct Clipboard(());

impl Clipboard {
    /// With an owner window, that window receives WM_RENDERFORMAT for delayed formats.
    #[allow(clippy::not_unsafe_ptr_arg_deref)] // an HWND is an opaque handle, never dereferenced
    pub fn open(owner: HWND) -> io::Result<Self> {
        for _ in 0..50 {
            // Another app may hold the clipboard for a moment.
            if unsafe { OpenClipboard(owner) } != 0 {
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
        set_data(format, data)
    }

    pub fn put_text(&self, text: &str) -> io::Result<()> {
        set_data(UNICODE_TEXT, &utf16_bytes(text))
    }

    /// Announce a format without data; the owner window renders it on paste.
    pub fn put_delayed(&self, format: u32) {
        unsafe { SetClipboardData(format, null_mut()) };
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        unsafe { CloseClipboard() };
    }
}

fn utf16_bytes(text: &str) -> Vec<u8> {
    wide(text).iter().flat_map(|c| c.to_le_bytes()).collect()
}

fn set_data(format: u32, data: &[u8]) -> io::Result<()> {
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

/// Answer WM_RENDERFORMAT: the pasting app already holds the clipboard open.
pub fn render_text(text: &str) -> io::Result<()> {
    set_data(UNICODE_TEXT, &utf16_bytes(text))
}

/// (window class, exe name) of the window being pasted into.
pub fn foreground() -> (String, String) {
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut class = [0u16; 256];
        GetClassNameW(hwnd, class.as_mut_ptr(), class.len() as i32);
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let mut exe = [0u16; 1024];
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if !process.is_null() {
            let mut len = exe.len() as u32;
            QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, exe.as_mut_ptr(), &mut len);
            CloseHandle(process);
        }
        let exe = from_wide(&exe);
        let name = exe.rsplit(['\\', '/']).next().unwrap_or("").to_owned();
        (from_wide(&class), name)
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

/// A named mutex held for the life of the watcher: one watcher per user session.
pub struct Mutex(HANDLE);

impl Mutex {
    /// None if another process already holds it.
    pub fn acquire(name: &str) -> Option<Mutex> {
        unsafe {
            let handle = CreateMutexW(null(), 0, wide(name).as_ptr());
            if handle.is_null() || GetLastError() == ERROR_ALREADY_EXISTS {
                if !handle.is_null() {
                    CloseHandle(handle);
                }
                return None;
            }
            Some(Mutex(handle))
        }
    }

    pub fn exists(name: &str) -> bool {
        unsafe {
            let handle = OpenMutexW(SYNCHRONIZE, 0, wide(name).as_ptr());
            if handle.is_null() {
                return false;
            }
            CloseHandle(handle);
            true
        }
    }
}

impl Drop for Mutex {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

// HKEY_CURRENT_USER values. Errors other than "missing" are rare enough to treat alike.

pub fn reg_get_string(subkey: &str, name: &str) -> Option<String> {
    let mut buf = [0u16; 2048];
    let mut size = (buf.len() * 2) as u32;
    let found = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, wide(subkey).as_ptr(), wide(name).as_ptr(), RRF_RT_REG_SZ, null_mut(),
                     buf.as_mut_ptr().cast(), &mut size)
    } == 0;
    found.then(|| from_wide(&buf))
}

pub fn reg_get_dword(subkey: &str, name: &str) -> Option<u32> {
    let (mut value, mut size) = (0u32, 4u32);
    let found = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, wide(subkey).as_ptr(), wide(name).as_ptr(), RRF_RT_REG_DWORD, null_mut(),
                     (&mut value as *mut u32).cast(), &mut size)
    } == 0;
    found.then_some(value)
}

pub fn reg_set_string(subkey: &str, name: &str, value: &str) -> io::Result<()> {
    let data = wide(value);
    let status = unsafe {
        RegSetKeyValueW(HKEY_CURRENT_USER, wide(subkey).as_ptr(), wide(name).as_ptr(), REG_SZ,
                        data.as_ptr().cast(), (data.len() * 2) as u32)
    };
    if status == 0 { Ok(()) } else { Err(io::Error::from_raw_os_error(status as i32)) }
}

pub fn reg_set_dword(subkey: &str, name: &str, value: u32) -> io::Result<()> {
    let status = unsafe {
        RegSetKeyValueW(HKEY_CURRENT_USER, wide(subkey).as_ptr(), wide(name).as_ptr(), REG_DWORD,
                        (&value as *const u32).cast(), 4)
    };
    if status == 0 { Ok(()) } else { Err(io::Error::from_raw_os_error(status as i32)) }
}

/// Deleting a value that is not there is not an error.
pub fn reg_delete(subkey: &str, name: &str) {
    unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, wide(subkey).as_ptr(), wide(name).as_ptr()) };
}

pub fn reg_subkeys(subkey: &str) -> Vec<String> {
    let mut names = Vec::new();
    unsafe {
        let mut key: HKEY = null_mut();
        if RegOpenKeyExW(HKEY_CURRENT_USER, wide(subkey).as_ptr(), 0, KEY_READ, &mut key) != 0 {
            return names;
        }
        for i in 0.. {
            let mut buf = [0u16; 256];
            let mut len = buf.len() as u32;
            if RegEnumKeyExW(key, i, buf.as_mut_ptr(), &mut len, null(), null_mut(), null_mut(), null_mut()) != 0 {
                break;
            }
            names.push(from_wide(&buf));
        }
        RegCloseKey(key);
    }
    names
}
