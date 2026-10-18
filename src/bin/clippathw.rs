//! `clippathw.exe`: the watcher alone, with no console window, so `clippath start` can
//! run it at login without one appearing. Messages go to %LOCALAPPDATA%\clippath\watch.log.
#![windows_subsystem = "windows"]

#[cfg(windows)]
fn main() {
    use std::io::Write;

    let wsl = std::env::args().any(|a| a == "--wsl");
    let log_file = clippath::win::folder().ok().map(|d| d.join("watch.log"));
    let log = move |line: &str| {
        if let Some(path) = &log_file
            && let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path)
        {
            let _ = writeln!(f, "{line}");
        }
    };
    let log = Box::new(log);
    if let Err(e) = clippath::watch::run(wsl, log.clone()) {
        log(&format!("clippath: {e}"));
    }
}

#[cfg(not(windows))]
fn main() {}
