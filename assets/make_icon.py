import io, sys
from PIL import Image, ImageDraw

S = 4  # supersample
N = 256 * S


def draw():
    im = Image.new("RGBA", (N, N), (0, 0, 0, 0))
    # vertical gradient background in a rounded square
    bg = Image.new("RGBA", (N, N))
    top, bot = (59, 130, 246), (29, 78, 216)
    for y in range(N):
        t = y / (N - 1)
        bg.paste(tuple(round(a + (b - a) * t) for a, b in zip(top, bot)) + (255,), (0, y, N, y + 1))
    mask = Image.new("L", (N, N))
    ImageDraw.Draw(mask).rounded_rectangle((8 * S, 8 * S, 248 * S, 248 * S), 56 * S, fill=255)
    im.paste(bg, (0, 0), mask)
    d = ImageDraw.Draw(im)
    s = lambda *v: [x * S for x in v]
    d.rounded_rectangle(s(60, 52, 196, 220), 18 * S, fill=(255, 255, 255, 255))  # board
    d.rounded_rectangle(s(94, 34, 162, 76), 14 * S, fill=(30, 58, 138, 255))  # clip
    d.line(s(110, 192, 146, 104), fill=(37, 99, 235, 255), width=24 * S)  # "/" = path
    for x, y in ((110, 192), (146, 104)):
        d.ellipse(s(x - 12, y - 12, x + 12, y + 12), fill=(37, 99, 235, 255))
    return im


big = draw()
sizes = [16, 24, 32, 48, 256]
ico = [16, 24, 32, 48]  # embedded in clippath.py; 256 not needed for tray / Task Manager
frames = {n: big.resize((n, n), Image.LANCZOS) for n in sizes}
out = sys.argv[1]
frames[256].save(out + "/logo.png")
frames[48].save(out + "/../clippath/clippath.ico", sizes=[(n, n) for n in ico], append_images=[frames[n] for n in ico[:-1]])

