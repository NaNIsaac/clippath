//! The Win32 calls clippath needs: the clipboard, the paste target, a few known
//! folders, the single-instance mutex and the registry.

use std::io;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::thread::sleep;
use std::time::Duration;

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, GetLastError, GlobalFree, HANDLE, HWND,
    INVALID_HANDLE_VALUE, LPARAM, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::core::BOOL;
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
    CREATE_NEW_PROCESS_GROUP, CreateMutexW, CreateProcessW, DETACHED_PROCESS, OpenMutexW, OpenProcess,
    PROCESS_INFORMATION, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    PROCESS_TERMINATE, QueryFullProcessImageNameW, STARTUPINFOW, TerminateProcess, WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::{FOLDERID_Screenshots, SHGetKnownFolderPath};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId, PostMessageW, WM_CLOSE,
};

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
        let exe = process_image(pid).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
        let name = exe.rsplit(['\\', '/']).next().unwrap_or("").to_owned();
        (from_wide(&class), name)
    }
}

/// The full path of a process's exe, if this process may query it.
pub fn process_image(pid: u32) -> Option<PathBuf> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) != 0;
        CloseHandle(process);
        ok.then(|| PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
    }
}

/// PIDs of the running processes whose exe file is called `name` (any case).
pub fn processes_named(name: &str) -> Vec<u32> {
    let mut pids = Vec::new();
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return pids;
        }
        let mut entry = PROCESSENTRY32W { dwSize: size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut more = Process32FirstW(snapshot, &mut entry) != 0;
        while more {
            if from_wide(&entry.szExeFile).eq_ignore_ascii_case(name) {
                pids.push(entry.th32ProcessID);
            }
            more = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    pids
}

/// Whether the process still runs. One we may not even wait on counts as running.
pub fn alive(pid: u32) -> bool {
    unsafe {
        let process = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if process.is_null() {
            return GetLastError() == ERROR_ACCESS_DENIED;
        }
        let running = WaitForSingleObject(process, 0) == WAIT_TIMEOUT;
        CloseHandle(process);
        running
    }
}

/// Ends a process at once and waits up to two seconds for it to be gone.
pub fn terminate(pid: u32) -> io::Result<()> {
    unsafe {
        let process = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, pid);
        if process.is_null() {
            return Err(io::Error::last_os_error());
        }
        let result = if TerminateProcess(process, 1) == 0 {
            Err(io::Error::last_os_error())
        } else if WaitForSingleObject(process, 2000) != WAIT_OBJECT_0 {
            Err(io::Error::other("still running after TerminateProcess"))
        } else {
            Ok(())
        };
        CloseHandle(process);
        result
    }
}

/// Top-level windows of a class, with the PID that owns each.
pub fn windows_of_class(class: &str) -> Vec<(HWND, u32)> {
    unsafe extern "system" fn each(hwnd: HWND, state: LPARAM) -> BOOL {
        unsafe {
            let (class, found) = &mut *(state as *mut (Vec<u16>, Vec<(HWND, u32)>));
            let mut buf = [0u16; 256];
            let len = GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) as usize;
            if buf[..len] == class[..class.len() - 1] {
                let mut pid = 0;
                GetWindowThreadProcessId(hwnd, &mut pid);
                found.push((hwnd, pid));
            }
        }
        1
    }
    let mut state = (wide(class), Vec::new());
    unsafe { EnumWindows(Some(each), &mut state as *mut _ as LPARAM) };
    state.1
}

/// Asks a window to close, as its close button would.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // an HWND is an opaque handle, never dereferenced
pub fn post_close(hwnd: HWND) -> io::Result<()> {
    // Fails with "access denied" when the window belongs to a process running as administrator.
    if unsafe { PostMessageW(hwnd, WM_CLOSE, 0, 0) } == 0 { Err(io::Error::last_os_error()) } else { Ok(()) }
}

/// Starts `exe` fully detached and returns its PID without waiting for it: no console,
/// its own process group (so Ctrl+C in the caller's console never reaches it), no
/// standard handles and no handles inherited from this process. `Command::spawn` passes
/// every inheritable handle on, including the pipe this process writes to when a shell
/// captures its output; a long-lived child would then hold that pipe open and the shell
/// would wait for it and never give its prompt back.
pub fn spawn_detached(exe: &Path, args: &[&str]) -> io::Result<u32> {
    let quote = |s: &str| {
        if s.is_empty() || s.contains([' ', '\t', '"']) { format!("\"{}\"", s.replace('"', "\\\"")) } else { s.to_owned() }
    };
    let exe_str = exe.to_string_lossy();
    let line = std::iter::once(format!("\"{exe_str}\"")).chain(args.iter().map(|a| quote(a))).collect::<Vec<_>>();
    let mut line = wide(&line.join(" ")); // CreateProcessW may write to the command line
    // Start in the exe's folder, so the child keeps no handle on the caller's directory.
    let dir = exe.parent().map(|d| wide(&d.to_string_lossy()));
    unsafe {
        let startup = STARTUPINFOW { cb: size_of::<STARTUPINFOW>() as u32, ..std::mem::zeroed() };
        let mut info: PROCESS_INFORMATION = std::mem::zeroed();
        let ok = CreateProcessW(
            wide(&exe_str).as_ptr(),
            line.as_mut_ptr(),
            null(),
            null(),
            0, // inherit no handles
            DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP,
            null(),
            dir.as_ref().map_or(null(), |d| d.as_ptr()),
            &startup,
            &mut info,
        );
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
        CloseHandle(info.hThread);
        CloseHandle(info.hProcess);
        Ok(info.dwProcessId)
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
                // Held by a process running as administrator: there, only not ours to open.
                return GetLastError() == ERROR_ACCESS_DENIED;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::time::Instant;

    const ROLE: &str = "CLIPPATH_TEST_ROLE";

    fn same(a: &Path, b: &Path) -> bool {
        let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_owned());
        canon(a) == canon(b)
    }

    #[test]
    fn finds_own_process() {
        let me = std::process::id();
        let exe = std::env::current_exe().unwrap();
        assert!(same(&process_image(me).unwrap(), &exe));
        let name = exe.file_name().unwrap().to_str().unwrap().to_uppercase();
        assert!(processes_named(&name).contains(&me));
        assert!(alive(me));
    }

    #[test]
    fn mutex_exists_while_held() {
        let name = format!("clippath-test-{}", std::process::id());
        assert!(!Mutex::exists(&name));
        let held = Mutex::acquire(&name).unwrap();
        assert!(Mutex::exists(&name));
        assert!(Mutex::acquire(&name).is_none());
        drop(held);
        assert!(!Mutex::exists(&name));
    }

    /// Run by `detached_child_releases_callers_pipe` as the caller: starts a long-lived
    /// child the way `clippath start` starts the watcher, prints its PID and exits.
    #[test]
    #[ignore = "helper process for detached_child_releases_callers_pipe"]
    fn helper_spawn_detached() {
        if std::env::var(ROLE).as_deref() != Ok("caller") {
            return;
        }
        let exe = std::env::current_exe().unwrap();
        // The test harness inherits stdout/stderr pipes from the test; the child must not.
        unsafe { std::env::set_var(ROLE, "child") };
        let pid = spawn_detached(&exe, &["--exact", "win::tests::helper_sleep", "--ignored"]).unwrap();
        println!("pid={pid}");
    }

    #[test]
    #[ignore = "helper process for detached_child_releases_callers_pipe"]
    fn helper_sleep() {
        if std::env::var(ROLE).as_deref() == Ok("child") {
            sleep(Duration::from_secs(20));
        }
    }

    #[test]
    fn detached_child_releases_callers_pipe() {
        let exe = std::env::current_exe().unwrap();
        let mut caller = Command::new(&exe)
            .args(["--exact", "win::tests::helper_spawn_detached", "--ignored", "--nocapture", "--test-threads=1"])
            .env(ROLE, "caller")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut stdout = caller.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut out = String::new();
            let _ = stdout.read_to_string(&mut out);
            let _ = tx.send(out);
        });
        let started = Instant::now();
        // EOF arrives once every holder of the pipe's write end has exited or closed it.
        let out = rx.recv_timeout(Duration::from_secs(15)).expect("caller's stdout never reached EOF");
        caller.wait().unwrap();
        let pid: u32 = out
            .lines()
            .find_map(|l| l.split_once("pid=").map(|(_, p)| p.trim()))
            .and_then(|p| p.parse().ok())
            .unwrap_or_else(|| panic!("no pid in caller output: {out}"));
        let child_alive = alive(pid);
        let _ = terminate(pid);
        assert!(child_alive, "detached child exited early");
        assert!(started.elapsed() < Duration::from_secs(15));
    }
}
