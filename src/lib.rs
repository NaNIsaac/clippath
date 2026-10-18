//! clippath: clipboard image -> PNG file -> path on the clipboard.
//!
//! Two programs share this library: `clippath.exe`, the command line, and
//! `clippathw.exe`, which runs only the watcher and has no console window, so it can
//! start at login without one appearing (the same split as python.exe / pythonw.exe).

pub mod image;
#[cfg(windows)]
pub mod capture;
#[cfg(windows)]
pub mod service;
#[cfg(windows)]
pub mod watch;
#[cfg(windows)]
pub mod win;
