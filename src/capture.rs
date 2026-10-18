//! Clipboard image -> PNG file, then the path next to the image on the clipboard.

use std::io;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use sha2::{Digest, Sha256};
use windows_sys::Win32::Foundation::HWND;

use crate::image;
use crate::win::{self, Clipboard};

/// One converted clipboard image, kept by the watcher to hand out again on paste.
pub struct Captured {
    pub path: PathBuf,
    /// The quoted path that goes on the clipboard.
    pub out: String,
    pub png: Vec<u8>,
    pub dib: Option<Vec<u8>>,
}

/// Saves the clipboard image as a PNG and puts the path next to it on the clipboard.
/// Ok(None) when the clipboard holds no image or changed meanwhile.
///
/// With an owner window the path is delay-rendered: the owner gets WM_RENDERFORMAT on
/// paste and decides then, from the paste target, whether to hand it out.
pub fn capture(wsl: bool, owner: HWND) -> Result<Option<Captured>, String> {
    let cf_png = win::register_format("PNG");
    let (png, dib, seq) = {
        let clip = Clipboard::open(std::ptr::null_mut()).map_err(|e| e.to_string())?;
        (clip.get(cf_png), clip.get(win::DIB), win::sequence())
    };
    let png = match (png, &dib) {
        // GlobalSize may pad past the end of the PNG.
        (Some(p), _) if p.windows(4).any(|w| w == b"IEND") => {
            let end = p.windows(4).rposition(|w| w == b"IEND").unwrap() + 8;
            p[..end.min(p.len())].to_vec()
        }
        (_, Some(d)) => image::dib_to_png(d)?,
        _ => return Ok(None),
    };

    // Reuse the screenshot tool's own file instead of keeping a second copy.
    let path = match find_screenshot(&png) {
        Some(p) => p,
        None => {
            let name: String = Sha256::digest(&png).iter().take(8).map(|b| format!("{b:02x}")).collect();
            let path = win::folder().map_err(|e| e.to_string())?.join(format!("{name}.png"));
            if !path.exists() {
                // Same image -> same file.
                std::fs::write(&path, &png).map_err(|e| e.to_string())?;
            }
            path
        }
    };
    let shown = path.to_string_lossy();
    let target = if wsl { image::to_wsl(&shown).ok_or_else(|| format!("no WSL path for {shown}"))? } else { shown.into_owned() };
    let captured = Captured { out: format!("\"{target}\""), path, png, dib }; // quoted: paths may contain spaces

    let clip = Clipboard::open(owner).map_err(|e| e.to_string())?;
    if win::sequence() != seq {
        return Ok(None); // the user copied something else meanwhile
    }
    offer(&clip, &captured, !owner.is_null(), true).map_err(|e| e.to_string())?;
    Ok(Some(captured))
}

/// Takes over the open clipboard: the path next to the image. `history: false` keeps a
/// repeat of the same image out of Win+V.
pub fn offer(clip: &Clipboard, c: &Captured, delay_text: bool, history: bool) -> io::Result<()> {
    clip.empty()?;
    if delay_text {
        clip.put_delayed(win::UNICODE_TEXT);
    } else {
        clip.put_text(&c.out)?;
    }
    clip.put(win::register_format("PNG"), &c.png)?;
    if let Some(d) = &c.dib {
        clip.put(win::DIB, d)?;
    }
    if !history {
        clip.put(win::register_format("CanIncludeInClipboardHistory"), &[0; 4])?;
    }
    Ok(())
}

/// The file a screenshot tool just saved for this image (Snipping Tool, Win+PrtScn), if
/// one of the three newest PNGs from the last 10 seconds has the same pixels.
pub fn find_screenshot(png: &[u8]) -> Option<PathBuf> {
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

// Apps that can't take an image get the path; everything else (browsers, messengers)
// gets only the image.
const PATH_CLASSES: &[&str] = &["ConsoleWindowClass", "CASCADIA_HOSTING_WINDOW_CLASS", "PseudoConsoleWindow"];
const PATH_APPS: &[&str] = &[
    "windowsterminal.exe", "openconsole.exe", "cmd.exe", "powershell.exe", "pwsh.exe", "wsl.exe",
    "wezterm-gui.exe", "alacritty.exe", "mintty.exe", "tabby.exe", "hyper.exe", "warp.exe",
    "conemu.exe", "conemu64.exe", "code.exe", "cursor.exe", "windsurf.exe", "zed.exe",
    "notepad.exe", "notepad++.exe", "sublime_text.exe", "explorer.exe",
];

pub fn takes_path(class: &str, exe: &str) -> bool {
    PATH_CLASSES.contains(&class) || PATH_APPS.contains(&exe.to_ascii_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::takes_path;

    #[test]
    fn terminals_and_editors_get_the_path_everything_else_the_image() {
        assert!(takes_path("CASCADIA_HOSTING_WINDOW_CLASS", "WindowsTerminal.exe"));
        assert!(takes_path("ConsoleWindowClass", "someshell.exe"));
        assert!(takes_path("Notepad", "Notepad.exe"));
        assert!(!takes_path("Chrome_WidgetWin_1", "chrome.exe"));
        assert!(!takes_path("EVA_Window_Dblclk", "KakaoTalk.exe"));
    }
}
