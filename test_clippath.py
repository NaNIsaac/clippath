import os
import struct
import tempfile
import time
import zlib

import clippath
from clippath import dib_to_png, find_screenshot, pixels, same_image, takes_path, to_wsl


def idat(png):
    i = png.index(b"IDAT")
    return zlib.decompress(png[i + 4: i + 4 + struct.unpack(">I", png[i - 4:i])[0]])


def test_dib_bottom_up_24bpp():
    # 2x2, stride 8; stored bottom row first, BGR
    hdr = struct.pack("<IiiHHIIiiII", 40, 2, 2, 1, 24, 0, 0, 0, 0, 0, 0)
    bottom = bytes([0, 0, 255, 0, 255, 0]) + b"\0\0"  # red, green
    top = bytes([255, 0, 0, 255, 255, 255]) + b"\0\0"  # blue, white
    raw = idat(dib_to_png(hdr + bottom + top))
    assert raw == b"\0" + bytes([0, 0, 255, 255, 255, 255]) + b"\0" + bytes([255, 0, 0, 0, 255, 0])


def test_dib_bitfields_32bpp_top_down():
    hdr = struct.pack("<IiiHHIIiiII", 40, 1, -1, 1, 32, 3, 0, 0, 0, 0, 0)
    masks = struct.pack("<III", 0xFF0000, 0xFF00, 0xFF)
    assert idat(dib_to_png(hdr + masks + bytes([1, 2, 3, 0]))) == b"\0" + bytes([3, 2, 1])


def test_to_wsl():
    assert to_wsl(r"C:\Users\me\shot.png") == "/mnt/c/Users/me/shot.png"



def test_takes_path():
    assert takes_path("CASCADIA_HOSTING_WINDOW_CLASS", "WindowsTerminal.exe")
    assert takes_path("ConsoleWindowClass", "someshell.exe")
    assert takes_path("Notepad", "Notepad.exe")
    assert not takes_path("Chrome_WidgetWin_1", "chrome.exe")
    assert not takes_path("EVA_Window_Dblclk", "KakaoTalk.exe")


def dib(rows):
    """Top-down 24bpp DIB from rows of (r, g, b) pixels."""
    width = len(rows[0])
    stride = (width * 24 + 31) // 32 * 4
    body = b"".join(bytes(c for r, g, b in row for c in (b, g, r)).ljust(stride, b"\0") for row in rows)
    return struct.pack("<IiiHHIIiiII", 40, width, -len(rows), 1, 24, 0, 0, 0, 0, 0, 0) + body


def png_like_a_screenshot_tool(rows):
    """Same pixels as dib_to_png(dib(rows)), different bytes: RGBA, Sub filter, level 9."""
    raw = b""
    for row in rows:
        line = bytes(c for r, g, b in row for c in (r, g, b, 255))
        raw += b"\1" + bytes((line[i] - (line[i - 4] if i >= 4 else 0)) & 0xFF for i in range(len(line)))

    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))

    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", len(rows[0]), len(rows), 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


SHOT = [[(255, 0, 0), (0, 255, 0), (0, 0, 255)], [(9, 9, 9), (200, 100, 50), (255, 255, 255)]]
OTHER = [[(255, 0, 0), (0, 255, 0), (0, 0, 255)], [(9, 9, 9), (200, 100, 51), (255, 255, 255)]]


def test_same_image_ignores_how_the_png_was_encoded():
    clip, tool = dib_to_png(dib(SHOT)), png_like_a_screenshot_tool(SHOT)
    assert clip != tool and same_image(tool, clip)


def test_same_size_different_pixels_is_not_the_same_image():
    assert not same_image(png_like_a_screenshot_tool(OTHER), dib_to_png(dib(SHOT)))


def test_pixels_of_something_that_is_not_an_image():
    assert pixels(b"\x89PNG not really") is None


def test_find_screenshot_reuses_only_a_file_with_the_same_pixels():
    clip = dib_to_png(dib(SHOT))
    with tempfile.TemporaryDirectory() as d:
        saved = clippath.screenshots_dir
        clippath.screenshots_dir = lambda: d
        try:
            with open(os.path.join(d, "other.png"), "wb") as f:
                f.write(png_like_a_screenshot_tool(OTHER))
            assert find_screenshot(clip) is None
            match = os.path.join(d, "shot.png")
            with open(match, "wb") as f:
                f.write(png_like_a_screenshot_tool(SHOT))
            os.utime(match, (time.time() + 1,) * 2)  # newest, like a fresh screenshot
            assert find_screenshot(clip) == match
        finally:
            clippath.screenshots_dir = saved


if __name__ == "__main__":
    for name, fn in list(globals().items()):
        if name.startswith("test_"):
            fn()
    print("ok")
