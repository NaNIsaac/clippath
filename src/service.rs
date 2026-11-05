//! Running the watcher in the background: start, stop, status, and cleaning up files.

use std::fmt;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use sha2::{Digest, Sha256};

use crate::watch::{CLASS, MUTEX};
use crate::win;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "clippath";
const TRAY_SETTINGS: &str = r"Control Panel\NotifyIconSettings";
const WATCHER_EXE: &str = "clippathw.exe";
const ERROR_SHARING_VIOLATION: i32 = 32;

pub fn running() -> bool {
    win::Mutex::exists(MUTEX)
}

/// The private copy that `start` runs and registers at login.
fn bin_exe() -> io::Result<PathBuf> {
    Ok(win::folder()?.join("bin").join(WATCHER_EXE))
}

/// Same file, ignoring case and other spellings of a path that exists.
fn same_path(a: &Path, b: &Path) -> bool {
    let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_owned());
    canon(a).as_os_str().eq_ignore_ascii_case(canon(b).as_os_str())
}

fn sha256(path: &Path) -> io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hasher.finalize().to_vec());
        }
        hasher.update(&buf[..n]);
    }
}

/// True when both files exist with the same size and SHA-256.
fn same_content(a: &Path, b: &Path) -> bool {
    match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(ma), Ok(mb)) if ma.len() == mb.len() => matches!((sha256(a), sha256(b)), (Ok(x), Ok(y)) if x == y),
        _ => false,
    }
}

/// What to do with a watcher that is still running after it was asked to close.
#[derive(Debug, PartialEq)]
enum Decision {
    /// Its exe is our private copy, so it is ours: end it.
    Terminate,
    /// Not provably ours: leave it, and say why.
    Leave(&'static str),
}

fn decide(image: Option<&Path>, bin: Option<&Path>) -> Decision {
    match (image, bin) {
        (None, _) => Decision::Leave("its path cannot be read (it may run as administrator)"),
        (Some(image), Some(bin)) if same_path(image, bin) => Decision::Terminate,
        (Some(_), _) => Decision::Leave("it is not the copy that `clippath start` runs"),
    }
}

/// A watcher that is still running after `stop`.
#[derive(Debug)]
pub struct Left {
    pub pid: u32,
    pub image: Option<PathBuf>,
    pub reason: String,
}

/// What `stop` did, one entry per watcher process it found.
#[derive(Debug, Default)]
pub struct StopReport {
    /// Closed when asked, which also removes their tray icon.
    pub closed: Vec<u32>,
    /// Did not close when asked; ended because their exe is our private copy.
    pub terminated: Vec<u32>,
    pub left: Vec<Left>,
}

impl StopReport {
    pub fn all_stopped(&self) -> bool {
        self.left.is_empty()
    }
}

impl fmt::Display for StopReport {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut lines = Vec::new();
        if self.closed.is_empty() && self.terminated.is_empty() && self.left.is_empty() {
            lines.push("No watcher was running.".to_string());
        }
        for pid in &self.closed {
            lines.push(format!("Stopped watcher (PID {pid})."));
        }
        for pid in &self.terminated {
            lines.push(format!("Ended watcher (PID {pid}): it did not close when asked."));
        }
        for l in &self.left {
            let path = l.image.as_ref().map_or("path unknown".into(), |p| p.display().to_string());
            lines.push(format!(
                "Still running: watcher PID {pid} ({path}), not stopped: {}.\n  \
                 To stop it, run `taskkill /PID {pid} /F` in a terminal opened as administrator, or sign out.",
                l.reason,
                pid = l.pid,
            ));
        }
        write!(f, "{}", lines.join("\n"))
    }
}

/// Stops every watcher: each window of our class (the Python watcher's too) is asked to
/// close, which removes its tray icon. A `clippathw.exe` still running after that is
/// ended only when its exe is provably our private copy; any other is reported and left
/// alone. Also removes the watcher from login.
pub fn stop() -> StopReport {
    let bin = bin_exe().ok();
    let mut pids = win::processes_named(WATCHER_EXE);
    let mut refused = Vec::new(); // windows we may not post to, e.g. of an elevated process
    for (hwnd, pid) in win::windows_of_class(CLASS) {
        if !pids.contains(&pid) {
            pids.push(pid);
        }
        if let Err(e) = win::post_close(hwnd) {
            refused.push((pid, e));
        }
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while pids.iter().any(|&p| win::alive(p)) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let mut report = StopReport::default();
    for pid in pids {
        if !win::alive(pid) {
            report.closed.push(pid);
            continue;
        }
        let image = win::process_image(pid);
        match decide(image.as_deref(), bin.as_deref()) {
            Decision::Terminate => match win::terminate(pid) {
                Ok(()) => report.terminated.push(pid),
                Err(e) => report.left.push(Left { pid, image, reason: format!("ending it failed: {e}") }),
            },
            Decision::Leave(why) => {
                let reason = match refused.iter().find(|(p, _)| *p == pid) {
                    Some((_, e)) => format!("{why}; asking it to close failed: {e}"),
                    None => format!("{why}; it did not close when asked"),
                };
                report.left.push(Left { pid, image, reason });
            }
        }
    }
    win::reg_delete(RUN_KEY, RUN_VALUE);
    report
}

/// Copies the watcher into the private bin folder, unless the copy there is already the
/// same file. A locked file is retried briefly: an exe stays locked for a moment after
/// its process ends.
fn install_copy(source: &Path, dest: &Path) -> Result<(), String> {
    if same_path(source, dest) || same_content(source, dest) {
        return Ok(());
    }
    for _ in 0..20 {
        match std::fs::copy(source, dest) {
            Ok(_) => return Ok(()),
            Err(e) if e.raw_os_error() == Some(ERROR_SHARING_VIOLATION) => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(format!("copy to {}: {e}", dest.display())),
        }
    }
    Err(locked_message(dest))
}

fn locked_message(dest: &Path) -> String {
    let holders: Vec<String> = win::processes_named(WATCHER_EXE)
        .into_iter()
        .filter(|&pid| win::process_image(pid).is_none_or(|image| same_path(&image, dest)))
        .map(|pid| pid.to_string())
        .collect();
    let by = if holders.is_empty() { "another process".into() } else { format!("PID {}", holders.join(", ")) };
    format!(
        "{} is a different version and is in use by {by}, so it cannot be updated.\n\
         Stop that process with `taskkill /PID <pid> /F` (in a terminal opened as administrator \
         if that is refused) or sign out, then run `clippath start` again.",
        dest.display()
    )
}

/// Starts `clippathw.exe` (the watcher without a console) now and at every login.
/// Returns once the watcher is launched; it never waits on it or shares a handle with it.
pub fn start(wsl: bool) -> Result<String, String> {
    // Restart cleanly, e.g. when switching to --wsl; also unlocks the copy below.
    let report = stop();
    if !report.all_stopped() {
        return Err(format!("{report}\nNot started: a watcher that could not be stopped is still running."));
    }
    let here = std::env::current_exe().map_err(|e| e.to_string())?;
    let source = here.with_file_name(WATCHER_EXE);
    if !source.is_file() {
        return Err(format!("{} not found; keep clippathw.exe next to clippath.exe", source.display()));
    }
    // A private copy, so moving or updating the downloaded exes never breaks autostart.
    let exe = bin_exe().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(exe.parent().expect("bin folder")).map_err(|e| e.to_string())?;
    install_copy(&source, &exe)?;
    let args: &[&str] = if wsl { &["--wsl"] } else { &[] };
    let command = std::iter::once(format!("\"{}\"", exe.display())).chain(args.iter().map(|a| a.to_string()));
    win::reg_set_string(RUN_KEY, RUN_VALUE, &command.collect::<Vec<_>>().join(" ")).map_err(|e| e.to_string())?;
    win::spawn_detached(&exe, args).map_err(|e| format!("start {}: {e}", exe.display()))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("clippath-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn same_content_compares_size_and_hash() {
        let dir = temp_dir("content");
        let (a, b, c, d) = (dir.join("a"), dir.join("b"), dir.join("c"), dir.join("d"));
        std::fs::write(&a, b"watcher v1").unwrap();
        std::fs::write(&b, b"watcher v1").unwrap();
        std::fs::write(&c, b"watcher v2").unwrap(); // same size, other bytes
        std::fs::write(&d, b"watcher v10").unwrap();
        assert!(same_content(&a, &b));
        assert!(!same_content(&a, &c));
        assert!(!same_content(&a, &d));
        assert!(!same_content(&a, &dir.join("missing")));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn install_copy_skips_identical_locked_file() {
        let dir = temp_dir("copy");
        let (source, dest) = (dir.join("new.exe"), dir.join("bin.exe"));
        std::fs::write(&source, b"same build").unwrap();
        std::fs::write(&dest, b"same build").unwrap();
        // Readable but not writable while held, as the exe of a running process is.
        let lock = std::os::windows::fs::OpenOptionsExt::share_mode(std::fs::OpenOptions::new().read(true), 1)
            .open(&dest)
            .unwrap();
        assert_eq!(install_copy(&source, &dest), Ok(()));
        // A different version over the locked file names the problem instead of a raw os error.
        std::fs::write(&source, b"other build").unwrap();
        let err = install_copy(&source, &dest).unwrap_err();
        assert!(err.contains("different version") && err.contains("clippath start"), "{err}");
        drop(lock);
        assert_eq!(install_copy(&source, &dest), Ok(()));
        assert_eq!(std::fs::read(&dest).unwrap(), b"other build");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn terminates_only_our_copy() {
        let dir = temp_dir("decide");
        let bin = dir.join("bin").join(WATCHER_EXE);
        std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
        std::fs::write(&bin, b"exe").unwrap();
        let upper = PathBuf::from(bin.to_string_lossy().to_uppercase());
        assert_eq!(decide(Some(&bin), Some(&bin)), Decision::Terminate);
        assert_eq!(decide(Some(&upper), Some(&bin)), Decision::Terminate);
        assert!(matches!(decide(None, Some(&bin)), Decision::Leave(_)));
        assert!(matches!(decide(Some(&dir.join(WATCHER_EXE)), Some(&bin)), Decision::Leave(_)));
        assert!(matches!(decide(Some(&bin), None), Decision::Leave(_)));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn report_says_what_was_and_was_not_stopped() {
        assert_eq!(StopReport::default().to_string(), "No watcher was running.");
        let report = StopReport {
            closed: vec![11],
            terminated: vec![22],
            left: vec![Left { pid: 33, image: None, reason: "its path cannot be read".into() }],
        };
        let text = report.to_string();
        assert!(!report.all_stopped());
        assert!(text.contains("Stopped watcher (PID 11)"), "{text}");
        assert!(text.contains("Ended watcher (PID 22)"), "{text}");
        assert!(text.contains("PID 33") && text.contains("taskkill /PID 33 /F"), "{text}");
    }
}
