"""clippath: clipboard image -> PNG file -> path on clipboard. Windows, stdlib only."""

import argparse
import contextlib
import ctypes
import hashlib
import os
import struct
import sys
import time
import uuid
import zlib
from ctypes import wintypes as w

H = ctypes.c_void_p
u32 = ctypes.WinDLL("user32", use_last_error=True)
k32 = ctypes.WinDLL("kernel32", use_last_error=True)
u32.OpenClipboard.argtypes = [w.HWND]
u32.GetClipboardData.argtypes, u32.GetClipboardData.restype = [w.UINT], H
u32.SetClipboardData.argtypes, u32.SetClipboardData.restype = [w.UINT, H], H
u32.RegisterClipboardFormatW.argtypes, u32.RegisterClipboardFormatW.restype = [w.LPCWSTR], w.UINT
k32.GlobalLock.argtypes, k32.GlobalLock.restype = [H], H
k32.GlobalUnlock.argtypes = [H]
k32.GlobalSize.argtypes, k32.GlobalSize.restype = [H], ctypes.c_size_t
k32.GlobalAlloc.argtypes, k32.GlobalAlloc.restype = [w.UINT, ctypes.c_size_t], H
k32.GlobalFree.argtypes = [H]
k32.CreateMutexW.argtypes, k32.CreateMutexW.restype = [H, w.BOOL, w.LPCWSTR], H
k32.OpenMutexW.argtypes, k32.OpenMutexW.restype = [w.DWORD, w.BOOL, w.LPCWSTR], H
k32.CloseHandle.argtypes = [H]


def fn(dll, name, res, *args):
    f = getattr(dll, name)
    f.restype, f.argtypes = res, list(args)
    return f


# tray window
s32 = ctypes.WinDLL("shell32", use_last_error=True)
WNDPROC = ctypes.WINFUNCTYPE(ctypes.c_ssize_t, w.HWND, w.UINT, w.WPARAM, w.LPARAM)
fn(u32, "DefWindowProcW", ctypes.c_ssize_t, w.HWND, w.UINT, w.WPARAM, w.LPARAM)
fn(
    u32,
    "CreateWindowExW",
    w.HWND,
    w.DWORD,
    w.LPCWSTR,
    w.LPCWSTR,
    w.DWORD,
    ctypes.c_int,
    ctypes.c_int,
    ctypes.c_int,
    ctypes.c_int,
    w.HWND,
    H,
    H,
    H,
)
fn(u32, "FindWindowW", w.HWND, w.LPCWSTR, w.LPCWSTR)
fn(u32, "PostMessageW", w.BOOL, w.HWND, w.UINT, w.WPARAM, w.LPARAM)
fn(u32, "DestroyWindow", w.BOOL, w.HWND)
fn(u32, "AddClipboardFormatListener", w.BOOL, w.HWND)
fn(u32, "SetTimer", ctypes.c_size_t, w.HWND, ctypes.c_size_t, w.UINT, H)
fn(u32, "KillTimer", w.BOOL, w.HWND, ctypes.c_size_t)
fn(u32, "GetMessageW", w.BOOL, ctypes.POINTER(w.MSG), w.HWND, w.UINT, w.UINT)
fn(u32, "TranslateMessage", w.BOOL, ctypes.POINTER(w.MSG))
fn(u32, "DispatchMessageW", ctypes.c_ssize_t, ctypes.POINTER(w.MSG))
fn(u32, "LoadImageW", H, H, w.LPCWSTR, w.UINT, ctypes.c_int, ctypes.c_int, w.UINT)
fn(u32, "RegisterWindowMessageW", w.UINT, w.LPCWSTR)
fn(u32, "CreatePopupMenu", H)
fn(u32, "AppendMenuW", w.BOOL, H, w.UINT, ctypes.c_size_t, w.LPCWSTR)
fn(u32, "TrackPopupMenu", w.BOOL, H, w.UINT, ctypes.c_int, ctypes.c_int, ctypes.c_int, w.HWND, H)
fn(u32, "DestroyMenu", w.BOOL, H)
fn(u32, "SetForegroundWindow", w.BOOL, w.HWND)
fn(u32, "GetCursorPos", w.BOOL, ctypes.POINTER(w.POINT))
fn(k32, "GetModuleHandleW", H, w.LPCWSTR)
# paste target
fn(u32, "GetForegroundWindow", w.HWND)
fn(u32, "GetClipboardOwner", w.HWND)
fn(u32, "GetClassNameW", ctypes.c_int, w.HWND, w.LPWSTR, ctypes.c_int)
fn(u32, "GetWindowThreadProcessId", w.DWORD, w.HWND, ctypes.POINTER(w.DWORD))
fn(k32, "OpenProcess", H, w.DWORD, w.BOOL, w.DWORD)
fn(k32, "QueryFullProcessImageNameW", w.BOOL, H, w.DWORD, w.LPWSTR, ctypes.POINTER(w.DWORD))
# exe icon
fn(k32, "BeginUpdateResourceW", H, w.LPCWSTR, w.BOOL)
fn(k32, "UpdateResourceW", w.BOOL, H, H, H, w.WORD, ctypes.c_char_p, w.DWORD)
fn(k32, "EndUpdateResourceW", w.BOOL, H, w.BOOL)


class WNDCLASSW(ctypes.Structure):
    _fields_ = [
        ("style", w.UINT),
        ("lpfnWndProc", WNDPROC),
        ("cbClsExtra", ctypes.c_int),
        ("cbWndExtra", ctypes.c_int),
        ("hInstance", H),
        ("hIcon", H),
        ("hCursor", H),
        ("hbrBackground", H),
        ("lpszMenuName", w.LPCWSTR),
        ("lpszClassName", w.LPCWSTR),
    ]


class NOTIFYICONDATAW(ctypes.Structure):
    _fields_ = [
        ("cbSize", w.DWORD),
        ("hWnd", w.HWND),
        ("uID", w.UINT),
        ("uFlags", w.UINT),
        ("uCallbackMessage", w.UINT),
        ("hIcon", H),
        ("szTip", w.WCHAR * 128),
        ("dwState", w.DWORD),
        ("dwStateMask", w.DWORD),
        ("szInfo", w.WCHAR * 256),
        ("uVersion", w.UINT),
        ("szInfoTitle", w.WCHAR * 64),
        ("dwInfoFlags", w.DWORD),
        ("guidItem", ctypes.c_byte * 16),
        ("hBalloonIcon", H),
    ]


ole32 = ctypes.WinDLL("ole32")
fn(ole32, "CoTaskMemFree", None, H)
fn(s32, "SHGetKnownFolderPath", ctypes.c_long, ctypes.c_char_p, w.DWORD, H, ctypes.POINTER(H))
fn(u32, "RegisterClassW", w.ATOM, ctypes.POINTER(WNDCLASSW))
fn(s32, "Shell_NotifyIconW", w.BOOL, w.DWORD, ctypes.POINTER(NOTIFYICONDATAW))

CF_DIB, CF_UNICODETEXT, GMEM_MOVEABLE = 8, 13, 2
CF_PNG = u32.RegisterClipboardFormatW("PNG")
CF_NO_HISTORY = u32.RegisterClipboardFormatW("CanIncludeInClipboardHistory")  # DWORD 0 = skip
WM_DESTROY, WM_CLOSE, WM_TIMER, WM_CLIPBOARDUPDATE, WM_TRAY = 0x2, 0x10, 0x113, 0x31D, 0x8001
WM_LBUTTONUP, WM_RBUTTONUP, WM_RENDERFORMAT = 0x202, 0x205, 0x305
ICON = os.path.join(os.path.dirname(os.path.abspath(__file__)), "clippath.ico")
CLASS = "clippath"


@contextlib.contextmanager
def clipboard(owner=None):
    for _ in range(50):  # another app may hold it briefly
        if u32.OpenClipboard(owner):
            break
        time.sleep(0.02)
    else:
        raise OSError("clipboard busy")
    try:
        yield
    finally:
        u32.CloseClipboard()


def get(fmt):
    h = u32.GetClipboardData(fmt)
    if not h:
        return None
    p = k32.GlobalLock(h)
    try:
        return ctypes.string_at(p, k32.GlobalSize(h))
    finally:
        k32.GlobalUnlock(h)


def put(fmt, data):
    h = k32.GlobalAlloc(GMEM_MOVEABLE, len(data))
    ctypes.memmove(k32.GlobalLock(h), data, len(data))
    k32.GlobalUnlock(h)
    if not u32.SetClipboardData(fmt, h):
        k32.GlobalFree(h)
        raise ctypes.WinError(ctypes.get_last_error())


def dib_to_png(dib):
    size, width, height, _, bpp, comp = struct.unpack_from("<IiiHHI", dib)
    if bpp not in (24, 32) or comp not in (0, 3):
        raise ValueError(f"unsupported DIB: {bpp}bpp, compression {comp}")
    # BI_BITFIELDS masks sit at offset 40 for every header version
    if comp == 3 and struct.unpack_from("<III", dib, 40) != (0xFF0000, 0xFF00, 0xFF):
        raise ValueError("unsupported BI_BITFIELDS masks")
    off = size + (12 if comp == 3 and size == 40 else 0)
    stride, n, rows = (width * bpp + 31) // 32 * 4, bpp // 8, abs(height)
    if len(dib) < off + stride * rows:
        raise ValueError("truncated DIB")
    raw = bytearray()
    # ponytail: 32bpp alpha dropped (CF_DIB alpha is unreliable); the PNG format keeps it when present
    for y in range(rows) if height < 0 else range(rows - 1, -1, -1):
        src = dib[off + y * stride : off + y * stride + width * n]
        rgb = bytearray(width * 3)
        rgb[0::3], rgb[1::3], rgb[2::3] = src[2::n], src[1::n], src[0::n]
        raw += b"\0" + rgb

    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, rows, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(raw), 6))
        + chunk(b"IEND", b"")
    )


def to_wsl(path):
    drive, rest = os.path.splitdrive(path)
    if len(drive) != 2:
        raise ValueError(f"no WSL path for {path}")
    return f"/mnt/{drive[0].lower()}" + rest.replace("\\", "/")


def folder():
    d = os.path.join(os.environ["LOCALAPPDATA"], "clippath")
    os.makedirs(d, exist_ok=True)
    return d


def screenshots_dir():
    p = ctypes.c_void_p()
    folder_id = ctypes.create_string_buffer(
        uuid.UUID("b7bede81-df94-4682-a7d8-57a52620b86f").bytes_le
    )
    if s32.SHGetKnownFolderPath(folder_id, 0, None, ctypes.byref(p)):  # FOLDERID_Screenshots
        return None
    try:
        return ctypes.wstring_at(p.value)
    finally:
        ole32.CoTaskMemFree(p)


# Decoding for find_screenshot. GDI+ ships with Windows, so this stays stdlib-only.
gdip = ctypes.WinDLL("gdiplus")
shlwapi = ctypes.WinDLL("shlwapi")
PixelFormat24bppRGB, ImageLockModeRead = 0x21808, 1


class GdiplusStartupInput(ctypes.Structure):
    _fields_ = [
        ("GdiplusVersion", ctypes.c_uint32),
        ("DebugEventCallback", H),
        ("SuppressBackgroundThread", w.BOOL),
        ("SuppressExternalCodecs", w.BOOL),
    ]


class GpRect(ctypes.Structure):
    _fields_ = [
        ("X", ctypes.c_int),
        ("Y", ctypes.c_int),
        ("Width", ctypes.c_int),
        ("Height", ctypes.c_int),
    ]


class BitmapData(ctypes.Structure):
    _fields_ = [
        ("Width", w.UINT),
        ("Height", w.UINT),
        ("Stride", ctypes.c_int),
        ("PixelFormat", ctypes.c_int),
        ("Scan0", H),
        ("Reserved", ctypes.c_size_t),
    ]


fn(shlwapi, "SHCreateMemStream", H, ctypes.c_char_p, w.UINT)
fn(
    gdip,
    "GdiplusStartup",
    ctypes.c_int,
    ctypes.POINTER(ctypes.c_size_t),
    ctypes.POINTER(GdiplusStartupInput),
    H,
)
fn(gdip, "GdipCreateBitmapFromStream", ctypes.c_int, H, ctypes.POINTER(H))
fn(gdip, "GdipGetImageWidth", ctypes.c_int, H, ctypes.POINTER(w.UINT))
fn(gdip, "GdipGetImageHeight", ctypes.c_int, H, ctypes.POINTER(w.UINT))
fn(
    gdip,
    "GdipBitmapLockBits",
    ctypes.c_int,
    H,
    ctypes.POINTER(GpRect),
    w.UINT,
    ctypes.c_int,
    ctypes.POINTER(BitmapData),
)
fn(gdip, "GdipBitmapUnlockBits", ctypes.c_int, H, ctypes.POINTER(BitmapData))
fn(gdip, "GdipDisposeImage", ctypes.c_int, H)
_gdip_token = None


def pixels(png):
    """RGB rows of an encoded image, or None if GDI+ cannot decode it. Alpha is dropped,
    as dib_to_png does, so a tool's RGBA file and the clipboard's RGB copy compare equal."""
    global _gdip_token
    if _gdip_token is None:
        token = ctypes.c_size_t()
        if gdip.GdiplusStartup(ctypes.byref(token), ctypes.byref(GdiplusStartupInput(1)), None):
            return None
        _gdip_token = token
    stream = shlwapi.SHCreateMemStream(png, len(png))
    if not stream:
        return None
    bmp = H()
    try:
        if gdip.GdipCreateBitmapFromStream(stream, ctypes.byref(bmp)):
            return None
        width, height = w.UINT(), w.UINT()
        gdip.GdipGetImageWidth(bmp, ctypes.byref(width))
        gdip.GdipGetImageHeight(bmp, ctypes.byref(height))
        rect, data = GpRect(0, 0, width.value, height.value), BitmapData()
        if gdip.GdipBitmapLockBits(
            bmp, ctypes.byref(rect), ImageLockModeRead, PixelFormat24bppRGB, ctypes.byref(data)
        ):
            return None
        try:
            return b"".join(
                ctypes.string_at(data.Scan0 + y * data.Stride, width.value * 3)
                for y in range(height.value)
            )
        finally:
            gdip.GdipBitmapUnlockBits(bmp, ctypes.byref(data))
    finally:
        if bmp:
            gdip.GdipDisposeImage(bmp)  # before the stream: the bitmap reads from it
        release = ctypes.WINFUNCTYPE(ctypes.c_ulong, H)(
            ctypes.cast(stream, ctypes.POINTER(ctypes.POINTER(H)))[0][2]
        )
        release(stream)  # IUnknown::Release


def same_image(a, b, b_pixels=None):
    """Same picture, whatever encoder wrote each PNG. `b_pixels` caches pixels(b) across calls."""
    if a == b:
        return True
    if a[16:24] != b[16:24]:  # IHDR width + height
        return False
    pa = pixels(a)
    return pa is not None and pa == (b_pixels if b_pixels is not None else pixels(b))


def find_screenshot(png):
    """File the screenshot tool just saved for this image (Snipping Tool, Win+PrtScn), else None."""
    d = screenshots_dir()
    if not d or not os.path.isdir(d):
        return None
    clip_pixels = None
    for _ in range(20):  # the tool may write the file just after setting the clipboard
        files = sorted(os.scandir(d), key=lambda e: e.stat().st_mtime, reverse=True)[:3]
        for e in files:
            if e.name.lower().endswith(".png") and time.time() - e.stat().st_mtime < 10:
                with contextlib.suppress(OSError), open(e.path, "rb") as f:
                    data = f.read()
                    if data[16:24] == png[16:24]:
                        if clip_pixels is None:  # decode the clipboard image once, only when needed
                            clip_pixels = pixels(png)
                        if same_image(data, png, clip_pixels):
                            return e.path
        time.sleep(0.05)
    return None


# Apps that can't take an image get the path; everything else (browsers, messengers) only the image.
PATH_CLASSES = {"ConsoleWindowClass", "CASCADIA_HOSTING_WINDOW_CLASS", "PseudoConsoleWindow"}
PATH_APPS = {
    "windowsterminal.exe",
    "openconsole.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "wsl.exe",
    "wezterm-gui.exe",
    "alacritty.exe",
    "mintty.exe",
    "tabby.exe",
    "hyper.exe",
    "warp.exe",
    "conemu.exe",
    "conemu64.exe",
    "code.exe",
    "cursor.exe",
    "windsurf.exe",
    "zed.exe",
    "notepad.exe",
    "notepad++.exe",
    "sublime_text.exe",
    "explorer.exe",
}


def takes_path(cls, exe):
    return cls in PATH_CLASSES or exe.lower() in PATH_APPS


def foreground():
    """(window class, exe name) of the window being pasted into."""
    hwnd = u32.GetForegroundWindow()
    cls = ctypes.create_unicode_buffer(256)
    u32.GetClassNameW(hwnd, cls, 256)
    pid = w.DWORD()
    u32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
    exe, n = ctypes.create_unicode_buffer(1024), w.DWORD(1024)
    h = k32.OpenProcess(0x1000, False, pid)  # PROCESS_QUERY_LIMITED_INFORMATION
    if h:
        k32.QueryFullProcessImageNameW(h, 0, exe, ctypes.byref(n))
        k32.CloseHandle(h)
    return cls.value, os.path.basename(exe.value)


def capture(wsl, owner=None):
    """Clipboard image -> PNG file path + image back on the clipboard. None if no image.

    With an owner window the path is delay-rendered: the owner gets WM_RENDERFORMAT
    on paste and decides then, from the paste target, whether to hand it out."""
    with clipboard():
        png, dib = get(CF_PNG), get(CF_DIB)
        seq = u32.GetClipboardSequenceNumber()
    if png and b"IEND" in png:
        png = png[: png.rfind(b"IEND") + 8]  # GlobalSize may pad past the end
    elif dib:
        png = dib_to_png(dib)
    else:
        return None
    path = find_screenshot(png)  # reuse the tool's own file instead of a second copy
    if not path:
        path = os.path.join(folder(), hashlib.sha256(png).hexdigest()[:16] + ".png")
        if not os.path.exists(path):  # same image -> same file
            with open(path, "wb") as f:
                f.write(png)
    out = f'"{to_wsl(path) if wsl else path}"'  # quoted: paths may contain spaces
    with clipboard(owner):
        if u32.GetClipboardSequenceNumber() != seq:  # user copied something else meanwhile
            return None
        offer(out, png, dib, owner)
    return path, out, png, dib


def offer(out, png, dib, owner=None, history=True):
    """Take over the open clipboard: the path next to the image."""
    u32.EmptyClipboard()  # SetClipboardData needs ownership
    if owner:
        u32.SetClipboardData(CF_UNICODETEXT, None)
    else:
        put(CF_UNICODETEXT, (out + "\0").encode("utf-16-le"))
    put(CF_PNG, png)
    if dib:
        put(CF_DIB, dib)
    if not history:  # same image again, keep it out of Win+V
        put(CF_NO_HISTORY, b"\0\0\0\0")


def copy(wsl):
    r = capture(wsl)
    if not r:
        sys.exit("clippath: no image on clipboard")
    print(f"File:   {r[0]}\nCopied: {r[1]}")


def watch(wsl):
    if sys.stderr is None:  # pythonw has no console; without this, a crash leaves no trace
        sys.stdout = sys.stderr = open(
            os.path.join(folder(), "watch.log"), "a", encoding="utf-8", buffering=1
        )
    mutex = k32.CreateMutexW(None, False, "clippath-watch")  # one watcher per user session
    if not mutex or ctypes.get_last_error() == 183:  # ERROR_ALREADY_EXISTS
        sys.exit("clippath: already watching")
    tip = f"clippath - {'WSL' if wsl else 'Windows'} paths"
    print(f"Watching clipboard for images ({tip[11:]}). Ctrl+C to stop.")

    last = None  # (path, out, png, dib) of the image on the clipboard

    def on_clipboard(hwnd):
        nonlocal last
        # text present = already a path (ours) or a normal copy; leave it alone
        if u32.IsClipboardFormatAvailable(CF_DIB) and not u32.IsClipboardFormatAvailable(
            CF_UNICODETEXT
        ):
            try:
                r = capture(wsl, hwnd)
                if r:
                    last = r
                    print(f"Copied: {r[1]}")
            except (OSError, ValueError) as e:
                print(f"clippath: {e}")

    def menu(hwnd):
        m = u32.CreatePopupMenu()
        u32.AppendMenuW(m, 0x1, 0, tip)  # MF_GRAYED: title
        u32.AppendMenuW(m, 0x800, 0, None)  # MF_SEPARATOR
        u32.AppendMenuW(m, 0, 1, "Open folder")
        u32.AppendMenuW(m, 0, 2, "Quit until next login")
        pt = w.POINT()
        u32.GetCursorPos(ctypes.byref(pt))
        u32.SetForegroundWindow(hwnd)  # else the menu won't close on outside click
        cmd = u32.TrackPopupMenu(
            m, 0x180, pt.x, pt.y, 0, hwnd, None
        )  # TPM_RETURNCMD | TPM_NONOTIFY
        u32.DestroyMenu(m)
        if cmd == 1:
            os.startfile(folder())
        elif cmd == 2:
            u32.DestroyWindow(hwnd)

    taskbar_created = u32.RegisterWindowMessageW("TaskbarCreated")

    @WNDPROC
    def proc(hwnd, msg, wp, lp):
        if msg == WM_CLIPBOARDUPDATE:  # let the source app finish writing before we open it
            u32.SetTimer(hwnd, 2, 150, None)
        elif msg == WM_TIMER and wp == 2:
            u32.KillTimer(hwnd, 2)
            on_clipboard(hwnd)
        elif msg == WM_RENDERFORMAT:  # the pasting app holds the clipboard open
            if wp == CF_UNICODETEXT and last and takes_path(*foreground()):
                put(CF_UNICODETEXT, (last[1] + "\0").encode("utf-16-le"))
                u32.SetTimer(hwnd, 3, 300, None)
        elif msg == WM_TIMER and wp == 3:
            # a rendered path would stick and reach the next browser paste; reset to delayed
            u32.KillTimer(hwnd, 3)
            if u32.GetClipboardOwner() == hwnd:  # nobody copied since
                try:
                    with clipboard(hwnd):
                        offer(*last[1:], hwnd, history=False)
                except OSError as e:
                    print(f"clippath: {e}")
        elif msg == WM_TRAY and lp in (WM_LBUTTONUP, WM_RBUTTONUP):
            menu(hwnd)
        elif msg == taskbar_created:  # explorer restarted, icon is gone
            s32.Shell_NotifyIconW(0, ctypes.byref(nid))  # NIM_ADD
        elif msg == WM_DESTROY:
            u32.PostQuitMessage(0)
        else:
            return u32.DefWindowProcW(hwnd, msg, wp, lp)
        return 0

    hinst = k32.GetModuleHandleW(None)
    u32.RegisterClassW(
        ctypes.byref(WNDCLASSW(lpfnWndProc=proc, hInstance=hinst, lpszClassName=CLASS))
    )
    hwnd = u32.CreateWindowExW(
        0, CLASS, CLASS, 0, 0, 0, 0, 0, None, None, hinst, None
    )  # hidden, never shown
    u32.AddClipboardFormatListener(hwnd)
    u32.SetTimer(hwnd, 1, 500, None)  # wakes GetMessage so Ctrl+C reaches Python
    size = u32.GetSystemMetrics(49)  # SM_CXSMICON
    icon = u32.LoadImageW(None, ICON, 1, size, size, 0x10)  # IMAGE_ICON, LR_LOADFROMFILE
    nid = NOTIFYICONDATAW(
        cbSize=ctypes.sizeof(NOTIFYICONDATAW),
        hWnd=hwnd,
        uID=1,
        uFlags=0x7,
        uCallbackMessage=WM_TRAY,
        hIcon=icon,
        szTip=tip,
    )  # NIF_MESSAGE|ICON|TIP
    s32.Shell_NotifyIconW(0, ctypes.byref(nid))
    msg = w.MSG()
    try:
        while u32.GetMessageW(ctypes.byref(msg), None, 0, 0) > 0:
            u32.TranslateMessage(ctypes.byref(msg))
            u32.DispatchMessageW(ctypes.byref(msg))
    finally:
        s32.Shell_NotifyIconW(2, ctypes.byref(nid))  # NIM_DELETE


RUN_KEY = r"Software\Microsoft\Windows\CurrentVersion\Run"


def running():
    h = k32.OpenMutexW(0x00100000, False, "clippath-watch")  # SYNCHRONIZE
    if h:
        k32.CloseHandle(h)
    return bool(h)


def stop():
    import winreg

    hwnd = u32.FindWindowW(CLASS, None)
    if hwnd:
        u32.PostMessageW(hwnd, WM_CLOSE, 0, 0)  # graceful, so the tray icon is removed
    for _ in range(60):
        if not running():
            break
        time.sleep(0.05)
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_SET_VALUE) as k:
        with contextlib.suppress(FileNotFoundError):
            winreg.DeleteValue(k, "clippath")


def set_icon(exe):
    ico = open(ICON, "rb").read()
    n = struct.unpack_from("<HHH", ico)[2]
    h = k32.BeginUpdateResourceW(
        exe, True
    )  # drop pythonw's own icon/version: Task Manager shows "clippath.exe"
    if not h:
        raise ctypes.WinError(ctypes.get_last_error())
    group = struct.pack("<HHH", 0, 1, n)
    for i in range(n):
        wd, ht, cc, _, planes, bpp, size, off = struct.unpack_from("<BBBBHHII", ico, 6 + 16 * i)
        k32.UpdateResourceW(h, 3, i + 1, 0, ico[off : off + size], size)  # RT_ICON
        group += struct.pack("<BBBBHHIH", wd, ht, cc, 0, planes, bpp, size, i + 1)
    k32.UpdateResourceW(h, 14, 1, 0, group, len(group))  # RT_GROUP_ICON
    if not k32.EndUpdateResourceW(h, False):
        raise ctypes.WinError(ctypes.get_last_error())


def show_in_tray(exe):
    """Windows 11 hides new tray icons under ^; pin ours once, unless the user already chose."""
    import winreg

    root = r"Control Panel\NotifyIconSettings"
    for _ in range(40):  # explorer writes the entry shortly after the icon appears
        with contextlib.suppress(OSError), winreg.OpenKey(winreg.HKEY_CURRENT_USER, root) as r:
            for i in range(winreg.QueryInfoKey(r)[0]):
                with winreg.OpenKey(
                    r, winreg.EnumKey(r, i), 0, winreg.KEY_READ | winreg.KEY_SET_VALUE
                ) as k:
                    with contextlib.suppress(OSError):
                        if os.path.normcase(
                            winreg.QueryValueEx(k, "ExecutablePath")[0]
                        ) == os.path.normcase(exe):
                            try:
                                winreg.QueryValueEx(k, "IsPromoted")
                            except FileNotFoundError:
                                winreg.SetValueEx(k, "IsPromoted", 0, winreg.REG_DWORD, 1)
                            return
        time.sleep(0.05)


def start(wsl):
    import glob
    import shutil
    import subprocess
    import winreg

    stop()  # restart cleanly, e.g. when switching --wsl; also unlocks the exe below
    # ponytail: private pythonw copy named clippath.exe so Task Manager shows our name + icon
    # (uv's launcher exe can't be patched: its payload is appended). Rebuilt on every start.
    bin_dir = os.path.join(folder(), "bin")
    site = os.path.join(bin_dir, "Lib", "site-packages")
    os.makedirs(site, exist_ok=True)
    exe = os.path.join(bin_dir, "clippath.exe")
    shutil.copy2(os.path.join(sys.base_prefix, "pythonw.exe"), exe)
    for dll in glob.glob(os.path.join(sys.base_prefix, "*.dll")):
        shutil.copy2(dll, bin_dir)
    with open(os.path.join(bin_dir, "pyvenv.cfg"), "w") as f:
        f.write(f"home = {sys.base_prefix}\n")  # stdlib from the real Python
    with open(os.path.join(site, "clippath.pth"), "w") as f:
        f.write(os.path.dirname(os.path.dirname(os.path.abspath(__file__))) + "\n")  # this install
    set_icon(exe)
    args = [exe, "-m", "clippath", "watch"] + (["--wsl"] if wsl else [])
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY, 0, winreg.KEY_SET_VALUE) as k:
        winreg.SetValueEx(k, "clippath", 0, winreg.REG_SZ, subprocess.list2cmdline(args))
    subprocess.Popen(args)
    show_in_tray(exe)
    print(f"Started in background ({'WSL' if wsl else 'Windows'} paths). Runs at every login.")


def status():
    import winreg

    h = running()
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, RUN_KEY) as k:
            cmd = winreg.QueryValueEx(k, "clippath")[0]
    except FileNotFoundError:
        cmd = None
    files = [os.path.join(folder(), f) for f in os.listdir(folder()) if f.endswith(".png")]
    mb = sum(os.path.getsize(f) for f in files) / 1e6
    print(
        f"Watcher:   {'running' if h else 'stopped'}\n"
        f"Autostart: {('on, ' + ('WSL' if '--wsl' in cmd else 'Windows') + ' paths') if cmd else 'off'}\n"
        f"Folder:    {folder()} ({len(files)} files, {mb:.1f} MB)"
    )


def clean(days):
    cutoff, n = time.time() - days * 86400, 0
    for f in os.listdir(folder()):
        p = os.path.join(folder(), f)
        if f.endswith(".png") and os.path.getmtime(p) <= cutoff:
            os.remove(p)
            n += 1
    print(f"Removed {n} file(s).")


def main():
    if sys.platform != "win32":
        sys.exit("clippath: Windows only")
    from importlib.metadata import PackageNotFoundError, version

    try:
        ver = version("clippath")
    except PackageNotFoundError:
        ver = "dev"
    paths = argparse.ArgumentParser(add_help=False)
    g = paths.add_mutually_exclusive_group()
    g.add_argument("--wsl", action="store_true", help="copy /mnt/c/... paths")
    g.add_argument("--windows", action="store_true", help="copy C:\\... paths (default)")
    ap = argparse.ArgumentParser(
        prog="clippath",
        description="Screenshot once. Paste where you need it.",
        epilog="Quick start: run `clippath start`, then screenshot and Ctrl+V in your terminal.",
    )
    ap.add_argument("-V", "--version", action="version", version=f"clippath {ver}")
    sub = ap.add_subparsers(dest="command", metavar="<command>")
    sub.add_parser("start", parents=[paths], help="watch in background and at every login")
    sub.add_parser("stop", help="stop the background watcher and remove it from login")
    sub.add_parser("status", help="show watcher, autostart and saved files")
    sub.add_parser("watch", parents=[paths], help="watch in the foreground (Ctrl+C to stop)")
    sub.add_parser("copy", parents=[paths], help="convert the clipboard image once")
    c = sub.add_parser("clean", help="delete saved screenshots")
    c.add_argument(
        "--days", type=float, default=0, metavar="N", help="keep files newer than N days"
    )
    a = ap.parse_args()
    try:
        if a.command == "start":
            start(a.wsl)
        elif a.command == "stop":
            stop()
            print("Stopped. Autostart off.")
        elif a.command == "status":
            status()
        elif a.command == "watch":
            watch(a.wsl)
        elif a.command == "copy":
            copy(a.wsl)
        elif a.command == "clean":
            clean(a.days)
        else:
            ap.print_help()
    except (OSError, ValueError) as e:
        sys.exit(f"clippath: {e}")
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
