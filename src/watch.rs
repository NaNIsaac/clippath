//! The watcher: a hidden window that converts every new clipboard image, hands the
//! path out only to apps that take one (terminals, editors), and shows a tray icon.

use std::cell::RefCell;
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicPtr, Ordering};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::core::BOOL;
use windows_sys::Win32::System::Console::SetConsoleCtrlHandler;
use windows_sys::Win32::System::DataExchange::{AddClipboardFormatListener, GetClipboardOwner};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow, DispatchMessageW,
    GetCursorPos, GetMessageW, GetSystemMetrics, IMAGE_ICON, KillTimer, LoadImageW, MF_GRAYED, MF_SEPARATOR,
    MF_STRING, MSG, PostMessageW, PostQuitMessage, RegisterClassW, RegisterWindowMessageW, SM_CXSMICON,
    SetForegroundWindow, SetTimer, TPM_NONOTIFY, TPM_RETURNCMD, TrackPopupMenu, TranslateMessage, WM_APP,
    WM_CLIPBOARDUPDATE, WM_CLOSE, WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP, WM_RENDERFORMAT, WM_TIMER, WNDCLASSW,
};

use crate::capture::{self, Captured};
use crate::win::{self, Clipboard, wide};

pub const CLASS: &str = "clippath";
pub const MUTEX: &str = "clippath-watch";
const WM_TRAY: u32 = WM_APP + 1;
const TIMER_SETTLE: usize = 2; // let the source app finish writing before we read
const TIMER_RESET: usize = 3; // put the path back to delayed after a paste
const MENU_FOLDER: usize = 1;
const MENU_QUIT: usize = 2;

struct State {
    wsl: bool,
    tip: String,
    log: Box<dyn Fn(&str)>,
    last: Option<Captured>,
    nid: NOTIFYICONDATAW,
    taskbar_created: u32,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}
static WINDOW: AtomicPtr<c_void> = AtomicPtr::new(null_mut());

fn log(text: &str) {
    STATE.with_borrow(|s| if let Some(s) = s { (s.log)(text) });
}

/// Runs until the tray menu's Quit, `clippath stop`, Ctrl+C or logoff.
pub fn run(wsl: bool, log_line: Box<dyn Fn(&str)>) -> Result<(), String> {
    let _mutex = win::Mutex::acquire(MUTEX).ok_or("already watching")?;
    let mode = if wsl { "WSL" } else { "Windows" };
    let tip = format!("clippath - {mode} paths");
    log_line(&format!("Watching clipboard for images ({mode} paths). Ctrl+C to stop."));
    unsafe {
        let hinst = GetModuleHandleW(null());
        let class = wide(CLASS);
        let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: hinst, lpszClassName: class.as_ptr(), ..std::mem::zeroed() };
        RegisterClassW(&wc);
        // Hidden, never shown: it only receives clipboard and tray messages.
        let hwnd = CreateWindowExW(0, class.as_ptr(), class.as_ptr(), 0, 0, 0, 0, 0, null_mut(), null_mut(), hinst, null());
        if hwnd.is_null() {
            return Err(std::io::Error::last_os_error().to_string());
        }
        WINDOW.store(hwnd, Ordering::SeqCst);
        AddClipboardFormatListener(hwnd);
        SetConsoleCtrlHandler(Some(on_console_close), 1); // no-op without a console (clippathw)

        let size = GetSystemMetrics(SM_CXSMICON);
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 1;
        nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY;
        nid.hIcon = LoadImageW(hinst, 1 as _, IMAGE_ICON, size, size, 0); // resource 1, see build.rs
        for (dst, src) in nid.szTip.iter_mut().zip(tip.encode_utf16().take(127)) {
            *dst = src;
        }
        Shell_NotifyIconW(NIM_ADD, &nid);
        let taskbar_created = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
        STATE.set(Some(State { wsl, tip, log: log_line, last: None, nid, taskbar_created }));

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        STATE.with_borrow(|s| if let Some(s) = s { Shell_NotifyIconW(NIM_DELETE, &s.nid); });
    }
    Ok(())
}

/// Ctrl+C or closing the console: close the window so the tray icon is removed.
unsafe extern "system" fn on_console_close(_event: u32) -> BOOL {
    unsafe { PostMessageW(WINDOW.load(Ordering::SeqCst), WM_CLOSE, 0, 0) };
    1
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_CLIPBOARDUPDATE => {
                SetTimer(hwnd, TIMER_SETTLE, 150, None);
            }
            WM_TIMER if wp == TIMER_SETTLE => {
                KillTimer(hwnd, TIMER_SETTLE);
                on_clipboard(hwnd);
            }
            WM_RENDERFORMAT if wp as u32 == win::UNICODE_TEXT => render_path(hwnd),
            WM_TIMER if wp == TIMER_RESET => {
                KillTimer(hwnd, TIMER_RESET);
                reset_to_delayed(hwnd);
            }
            WM_TRAY if matches!(lp as u32, WM_LBUTTONUP | WM_RBUTTONUP) => menu(hwnd),
            WM_DESTROY => PostQuitMessage(0),
            _ if msg == STATE.with_borrow(|s| s.as_ref().map_or(0, |s| s.taskbar_created)) => {
                // Explorer restarted and the icon is gone.
                STATE.with_borrow(|s| if let Some(s) = s { Shell_NotifyIconW(NIM_ADD, &s.nid); });
            }
            _ => return DefWindowProcW(hwnd, msg, wp, lp),
        }
    }
    0
}

fn on_clipboard(hwnd: HWND) {
    // Text present means it is already a path (ours) or an ordinary copy: leave it alone.
    if !win::has_format(win::DIB) || win::has_format(win::UNICODE_TEXT) {
        return;
    }
    let wsl = STATE.with_borrow(|s| s.as_ref().is_some_and(|s| s.wsl));
    // No borrow is held here: opening the clipboard can re-enter the window procedure.
    match capture::capture(wsl, hwnd) {
        Ok(Some(c)) => {
            log(&format!("Copied: {}", c.out));
            STATE.with_borrow_mut(|s| if let Some(s) = s { s.last = Some(c) });
        }
        Ok(None) => {}
        Err(e) => log(&format!("clippath: {e}")),
    }
}

fn render_path(hwnd: HWND) {
    let out = STATE.with_borrow(|s| s.as_ref().and_then(|s| s.last.as_ref().map(|c| c.out.clone())));
    let Some(out) = out else { return };
    let (class, exe) = win::foreground();
    if capture::takes_path(&class, &exe) {
        if let Err(e) = win::render_text(&out) {
            log(&format!("clippath: {e}"));
        }
        // A rendered path would stick and reach the next browser paste; undo it shortly.
        unsafe { SetTimer(hwnd, TIMER_RESET, 300, None) };
    }
}

fn reset_to_delayed(hwnd: HWND) {
    if unsafe { GetClipboardOwner() } != hwnd {
        return; // someone copied something else since
    }
    let result = STATE.with_borrow(|s| -> Result<(), String> {
        let Some(c) = s.as_ref().and_then(|s| s.last.as_ref()) else { return Ok(()) };
        let clip = Clipboard::open(hwnd).map_err(|e| e.to_string())?;
        capture::offer(&clip, c, true, false).map_err(|e| e.to_string())
    });
    if let Err(e) = result {
        log(&format!("clippath: {e}"));
    }
}

fn menu(hwnd: HWND) {
    let tip = STATE.with_borrow(|s| s.as_ref().map(|s| s.tip.clone()).unwrap_or_default());
    unsafe {
        let m = CreatePopupMenu();
        AppendMenuW(m, MF_GRAYED, 0, wide(&tip).as_ptr());
        AppendMenuW(m, MF_SEPARATOR, 0, null());
        AppendMenuW(m, MF_STRING, MENU_FOLDER, wide("Open folder").as_ptr());
        AppendMenuW(m, MF_STRING, MENU_QUIT, wide("Quit until next login").as_ptr());
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd); // else the menu does not close on an outside click
        let cmd = TrackPopupMenu(m, TPM_RETURNCMD | TPM_NONOTIFY, pt.x, pt.y, 0, hwnd, null()) as usize;
        DestroyMenu(m);
        match cmd {
            MENU_FOLDER => {
                if let Ok(dir) = win::folder() {
                    let _ = std::process::Command::new("explorer").arg(dir).spawn();
                }
            }
            MENU_QUIT => {
                DestroyWindow(hwnd);
            }
            _ => {}
        }
    }
}
