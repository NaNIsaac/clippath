//! Running the watcher in the background: start, stop, status, and cleaning up files.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW, WM_CLOSE};

use crate::watch::{CLASS, MUTEX};
use crate::win::{self, wide};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "clippath";
const TRAY_SETTINGS: &str = r"Control Panel\NotifyIconSettings";

pub fn running() -> bool {
    win::Mutex::exists(MUTEX)
}

/// Stops the watcher (the Python one too: same window class and mutex) and removes
/// it from login.
pub fn stop() {
    unsafe {
        let hwnd = FindWindowW(wide(CLASS).as_ptr(), std::ptr::null());
        if !hwnd.is_null() {
            PostMessageW(hwnd, WM_CLOSE, 0, 0); // graceful, so the tray icon is removed
        }
    }
    for _ in 0..60 {
        if !running() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    win::reg_delete(RUN_KEY, RUN_VALUE);
}

/// Starts `clippathw.exe` (the watcher without a console) now and at every login.
pub fn start(wsl: bool) -> Result<String, String> {
    stop(); // restart cleanly, e.g. when switching to --wsl; also unlocks the copy below
    let here = std::env::current_exe().map_err(|e| e.to_string())?;
    let source = here.with_file_name("clippathw.exe");
    if !source.is_file() {
        return Err(format!("{} not found; keep clippathw.exe next to clippath.exe", source.display()));
    }
    // A private copy, so moving or updating the downloaded exes never breaks autostart.
    let bin = win::folder().map_err(|e| e.to_string())?.join("bin");
    std::fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    let exe = bin.join("clippathw.exe");
    if source != exe {
        std::fs::copy(&source, &exe).map_err(|e| format!("copy to {}: {e}", exe.display()))?;
    }
    let args: &[&str] = if wsl { &["--wsl"] } else { &[] };
    let command = std::iter::once(format!("\"{}\"", exe.display())).chain(args.iter().map(|a| a.to_string()));
    win::reg_set_string(RUN_KEY, RUN_VALUE, &command.collect::<Vec<_>>().join(" ")).map_err(|e| e.to_string())?;
    Command::new(&exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    show_in_tray(&exe);
    Ok(format!("Started in background ({} paths). Runs at every login.", if wsl { "WSL" } else { "Windows" }))
}

/// Windows 11 hides new tray icons under ^; pin ours once, unless the user already chose.
fn show_in_tray(exe: &Path) {
    let exe = exe.to_string_lossy();
    for _ in 0..40 {
        // Explorer writes the entry shortly after the icon appears.
        for name in win::reg_subkeys(TRAY_SETTINGS) {
            let key = format!(r"{TRAY_SETTINGS}\{name}");
            if win::reg_get_string(&key, "ExecutablePath").is_some_and(|p| p.eq_ignore_ascii_case(&exe)) {
                if win::reg_get_dword(&key, "IsPromoted").is_none() {
                    let _ = win::reg_set_dword(&key, "IsPromoted", 1);
                }
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn saved_pngs() -> Result<Vec<(std::path::PathBuf, std::fs::Metadata)>, String> {
    let dir = win::folder().map_err(|e| e.to_string())?;
    Ok(std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("png")))
        .filter_map(|e| Some((e.path(), e.metadata().ok()?)))
        .collect())
}

pub fn status() -> Result<String, String> {
    let autostart = match win::reg_get_string(RUN_KEY, RUN_VALUE) {
        Some(cmd) => format!("on, {} paths", if cmd.contains("--wsl") { "WSL" } else { "Windows" }),
        None => "off".into(),
    };
    let files = saved_pngs()?;
    let mb = files.iter().map(|(_, m)| m.len()).sum::<u64>() as f64 / 1e6;
    Ok(format!(
        "Watcher:   {}\nAutostart: {autostart}\nFolder:    {} ({} files, {mb:.1} MB)",
        if running() { "running" } else { "stopped" },
        win::folder().map_err(|e| e.to_string())?.display(),
        files.len(),
    ))
}

/// Deletes saved PNGs older than `days` (all of them with 0). Returns how many.
pub fn clean(days: f64) -> Result<usize, String> {
    let cutoff = SystemTime::now() - Duration::from_secs_f64(days * 86400.0);
    let mut removed = 0;
    for (path, meta) in saved_pngs()? {
        if meta.modified().is_ok_and(|t| t <= cutoff) && std::fs::remove_file(&path).is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}
