import struct
import zlib

from clippath import dib_to_png, to_wsl


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


if __name__ == "__main__":
    for name, fn in list(globals().items()):
        if name.startswith("test_"):
            fn()
    print("ok")
