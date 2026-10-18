//! clippath in Rust: clipboard image -> PNG file -> path on the clipboard.
//!
//! This first milestone ports `clippath copy`. The background watcher, tray icon and
//! the other commands are still provided by the Python package.

mod image;
#[cfg(windows)]
mod win;

use std::process::ExitCode;

const USAGE: &str = "\
clippath: Screenshot once. Paste where you need it.

usage: clippath copy [--wsl | --windows]   convert the clipboard image once
       clippath -V | --version

Background watching (start, stop, status, watch) and clean are not in the Rust
version yet; the Python clippath still provides them.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let flags: Vec<&str> = args.iter().map(String::as_str).collect();
    match flags.as_slice() {
        ["-V" | "--version"] => println!("clippath {}", env!("CARGO_PKG_VERSION")),
        ["copy", rest @ ..] if rest.iter().all(|f| matches!(*f, "--wsl" | "--windows")) => {
            if let Err(e) = copy(rest.contains(&"--wsl")) {
                eprintln!("clippath: {e}");
                return ExitCode::FAILURE;
            }
        }
        [] | ["-h" | "--help"] => println!("{USAGE}"),
        [cmd @ ("start" | "stop" | "status" | "watch" | "clean"), ..] => {
            eprintln!("clippath: `{cmd}` is not in the Rust version yet; use the Python clippath for now");
            return ExitCode::from(2);
        }
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}

#[cfg(not(windows))]
fn copy(_wsl: bool) -> Result<(), String> {
    Err("Windows only".into())
}

#[cfg(windows)]
fn copy(wsl: bool) -> Result<(), String> {
    use sha2::{Digest, Sha256};

    let cf_png = win::register_format("PNG");
    let (png, dib, seq) = {
        let clip = win::Clipboard::open().map_err(|e| e.to_string())?;
        (clip.get(cf_png), clip.get(win::DIB), win::sequence())
    };
    let png = match (png, &dib) {
        // GlobalSize may pad past the end of the PNG.
        (Some(p), _) if p.windows(4).any(|w| w == b"IEND") => {
            let end = p.windows(4).rposition(|w| w == b"IEND").unwrap() + 8;
            p[..end.min(p.len())].to_vec()
        }
        (_, Some(d)) => image::dib_to_png(d)?,
        _ => return Err("no image on clipboard".into()),
    };

    // Reuse the screenshot tool's own file instead of keeping a second copy.
    let path = match find_screenshot(&png) {
        Some(p) => p,
        None => {
            let name = Sha256::digest(&png).iter().take(8).map(|b| format!("{b:02x}")).collect::<String>();
            let path = win::folder().map_err(|e| e.to_string())?.join(format!("{name}.png"));
            if !path.exists() {
                // Same image -> same file.
                std::fs::write(&path, &png).map_err(|e| e.to_string())?;
            }
            path
        }
    };
    let shown = path.to_string_lossy().into_owned();
    let target = if wsl { image::to_wsl(&shown).ok_or_else(|| format!("no WSL path for {shown}"))? } else { shown.clone() };
    let out = format!("\"{target}\""); // quoted: paths may contain spaces

    let clip = win::Clipboard::open().map_err(|e| e.to_string())?;
    if win::sequence() != seq {
        return Err("the clipboard changed meanwhile; nothing copied".into());
    }
    clip.empty().map_err(|e| e.to_string())?;
    clip.put_text(&out).map_err(|e| e.to_string())?;
    clip.put(cf_png, &png).map_err(|e| e.to_string())?;
    if let Some(d) = &dib {
        clip.put(win::DIB, d).map_err(|e| e.to_string())?;
    }
    println!("File:   {shown}\nCopied: {out}");
    Ok(())
}

/// The file a screenshot tool just saved for this image (Snipping Tool, Win+PrtScn), if
/// one of the three newest PNGs from the last 10 seconds has the same pixels.
#[cfg(windows)]
fn find_screenshot(png: &[u8]) -> Option<std::path::PathBuf> {
    use std::time::{Duration, SystemTime};

    let dir = win::screenshots_dir()?;
    let mut clip_pixels = None;
    for _ in 0..20 {
        // The tool may write the file just after setting the clipboard.
        let mut recent: Vec<_> = std::fs::read_dir(&dir)
            .ok()?
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("png")))
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .filter(|(t, _)| SystemTime::now().duration_since(*t).unwrap_or_default() < Duration::from_secs(10))
            .collect();
        recent.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
        for (_, path) in recent.into_iter().take(3) {
            let Ok(data) = std::fs::read(&path) else { continue };
            if data == png {
                return Some(path);
            }
            if image::png_size(&data) == image::png_size(png) {
                let clip = clip_pixels.get_or_insert_with(|| image::rgb_pixels(png));
                if clip.is_some() && image::rgb_pixels(&data) == *clip {
                    return Some(path);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}
