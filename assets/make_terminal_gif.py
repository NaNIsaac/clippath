from PIL import Image, ImageDraw, ImageFont

W, H, PAD, LH = 1240, 230, 24, 30
BG, FG, DIM, ACCENT, PATH = (
    (12, 12, 12),
    (204, 204, 204),
    (110, 110, 110),
    (97, 175, 239),
    (152, 195, 121),
)
font = ImageFont.truetype(r"C:\Windows\Fonts\CascadiaMono.ttf", 18)
badge_font = ImageFont.truetype(r"C:\Windows\Fonts\CascadiaMono.ttf", 20)
PROMPT = r"PS C:\code\site> "
SHOT = r'"C:\Users\me\Pictures\Screenshots\Screenshot 2026-10-17 101500.png"'
CW = font.getlength("M")

frames, durations = [], []


def frame(lines, ms, badge=None, dim=False, cursor=True):
    """lines: list of rows, each a list of (text, color) runs."""
    im = Image.new("RGB", (W, H), BG)
    d = ImageDraw.Draw(im)
    d.rectangle((0, 0, W, 34), fill=(32, 32, 32))
    d.text((PAD, 8), "Windows PowerShell", font=font, fill=DIM)
    y = 34 + PAD
    for row in lines:
        x = PAD
        for text, color in row:
            d.text((x, y), text, font=font, fill=color)
            x += CW * len(text)
        y += LH
    if cursor:
        d.rectangle((x, y - LH + 4, x + CW - 2, y - 6), fill=FG)
    if dim:  # Snipping Tool's overlay
        im = Image.blend(im, Image.new("RGB", (W, H), (0, 0, 0)), 0.55)
        d = ImageDraw.Draw(im)
    if badge:
        bw = badge_font.getlength(badge) + 28
        d.rounded_rectangle(
            (W - bw - PAD, H - 50, W - PAD, H - 14), 8, fill=(45, 45, 45), outline=ACCENT
        )
        d.text((W - bw - PAD + 14, H - 44), badge, font=badge_font, fill=(255, 255, 255))
    frames.append(im)
    durations.append(ms)


def typed(done, row, text, color=FG, ms=70):
    for i in range(1, len(text) + 1):
        frame(done + [row + [(text[:i], color)]], ms)
    return row + [(text, color)]


p = [(PROMPT, ACCENT)]
frame([p], 600)
row = typed([], p, "cp ")
frame([row], 500)
frame([row], 700, "Win+Shift+S", dim=True, cursor=False)
frame([row], 500, "Win+Shift+S", dim=True, cursor=False)
frame([row], 400)
row += [(SHOT, PATH)]
frame([row], 1600, "Ctrl+V")
row = typed([], row, r" images\bug.png")
frame([row], 500)
done = [row]
row = typed(done, p, "ls images -Name")
frame(done + [row], 400)
done += [row, [("bug.png", FG)]]
frame(done + [p], 3000)

frames[0].save(
    "terminal.gif",
    save_all=True,
    append_images=frames[1:],
    duration=durations,
    loop=0,
    optimize=True,
)
