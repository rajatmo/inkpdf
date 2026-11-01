"""Cuts the takes recorded by tour.py into the README tour: docs/demo.mp4 (1920 x 1080, 30 fps, H.264) and docs/demo.gif (960 wide).

Each take's frames are put on a dark stage in a window frame (the window shown one to one), with the chapter's caption beside it, the
mouse pointer and its clicks, the stylus tip and fingers, and a badge for each shortcut drawn from the take's log. A title card opens it,
an end card closes it, and the takes are joined with short cross-fades. Needs ffmpeg (the `imageio-ffmpeg` package carries one) and
the Segoe UI fonts Windows ships.

    python tools/demo/compose.py              docs/demo.mp4 and docs/demo.gif
    python tools/demo/compose.py --preview    every frame skipped but a few, as PNGs in the takes' folder, to look at
"""

import argparse
import bisect
import json
import math
import os
import subprocess
import sys

import cv2
import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
TAKES = os.environ.get("INKPDF_DEMO_DIR") or os.path.join(os.path.dirname(REPO), "inkpdf-demo")
ORDER = ["1-paper", "2-form", "3-present", "4-narrow"]
TITLES = {"1-paper": "Writing on PDFs with a Pen.pdf", "2-form": "Registration.pdf", "3-present": "Field Study.pdf",
          "4-narrow": "Writing on PDFs with a Pen.pdf"}

W, H, FPS = 1920, 1080, 30
FADE = 0.5  # seconds of cross-fade between takes

# The look: a deep blue stage, a light window, one accent.
BG_TOP, BG_BOTTOM = (13, 18, 33), (22, 30, 52)
TEXT, MUTED, ACCENT = (244, 246, 251), (150, 162, 186), (92, 145, 255)
FONTS = r"C:\Windows\Fonts"


def font(name, size):
    return ImageFont.truetype(os.path.join(FONTS, name), size)


F_TITLE = font("seguisb.ttf", 44)
F_SUB = font("segoeui.ttf", 26)
F_TAG = font("seguisb.ttf", 17)
F_BRAND = font("segoeuib.ttf", 30)
F_WINDOW = font("segoeui.ttf", 17)
F_KEY = font("seguisb.ttf", 26)


def ease(t):
    t = min(1.0, max(0.0, t))
    return t * t * (3 - 2 * t)


# ---- Fixed pictures
def stage_background():
    y = np.linspace(0, 1, H)[:, None, None]
    x = np.linspace(0, 1, W)[None, :, None]
    top, bottom = np.array(BG_TOP, float), np.array(BG_BOTTOM, float)
    img = top + (bottom - top) * (0.6 * y + 0.4 * x)
    # A soft glow behind the window, right of centre.
    gx, gy = 0.62, 0.45
    glow = np.exp(-(((x - gx) * 1.6) ** 2 + ((y - gy) * 2.2) ** 2) * 3.2)
    img += glow * np.array([30, 50, 95], float) * 0.55
    return Image.fromarray(np.clip(img, 0, 255).astype(np.uint8), "RGB")


def logo(size, colour=TEXT):
    # (The icon is dark lines on a white tile: its lines are what is kept, in `colour`.)
    src = Image.open(os.path.join(REPO, "apps", "inkpdf", "installer", "app-256.png")).convert("RGBA")
    a = np.asarray(src, float)
    ink = a[:, :, 3] * (1 - a[:, :, :3].mean(axis=2) / 255)
    alpha = Image.fromarray(ink.astype(np.uint8), "L").resize((size, size), Image.LANCZOS)
    out = Image.new("RGBA", (size, size), colour + (0,))
    out.putalpha(alpha)
    return out


def rounded_mask(w, h, r, scale=3):
    m = Image.new("L", (w * scale, h * scale), 0)
    ImageDraw.Draw(m).rounded_rectangle((0, 0, w * scale - 1, h * scale - 1), r * scale, fill=255)
    return m.resize((w, h), Image.LANCZOS)


def shadow(w, h, r, blur=38, spread=10, alpha=150, dy=18):
    pad = blur * 2 + spread
    s = Image.new("L", (w + pad * 2, h + pad * 2), 0)
    ImageDraw.Draw(s).rounded_rectangle((pad - spread, pad - spread + dy, pad + w + spread, pad + h + spread + dy), r + spread, fill=alpha)
    s = s.filter(ImageFilter.GaussianBlur(blur))
    out = Image.new("RGBA", s.size, (0, 0, 0, 0))
    out.putalpha(s)
    return out, pad


def cursor_sprite(scale=1.55):
    """An arrow pointer, white with a dark edge and a soft shadow."""
    k = 4
    pts = [(0, 0), (0, 17), (4.2, 13.1), (7.0, 19.6), (9.4, 18.6), (6.7, 12.3), (12.2, 12.3)]
    size = int(24 * scale)
    img = Image.new("RGBA", (size * k, size * k), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    poly = [((x * scale + 3) * k, (y * scale + 2) * k) for x, y in pts]
    d.polygon(poly, fill=(255, 255, 255, 255), outline=(20, 22, 28, 255), width=int(1.6 * k))
    img = img.resize((size, size), Image.LANCZOS)
    sh = Image.new("RGBA", img.size, (0, 0, 0, 0))
    sh.putalpha(img.getchannel("A").point(lambda a: a * 0.45).filter(ImageFilter.GaussianBlur(2.5)))
    out = Image.new("RGBA", (size + 4, size + 4), (0, 0, 0, 0))
    out.alpha_composite(sh, (3, 4))
    out.alpha_composite(img, (0, 0))
    return out, (3, 2)  # the tip's place in the picture


def wrap(text, f, width):
    words, lines, line = text.split(), [], ""
    for w in words:
        trial = (line + " " + w).strip()
        if f.getlength(trial) <= width or not line:
            line = trial
        else:
            lines.append(line)
            line = w
    if line:
        lines.append(line)
    return lines


def caption_card(title, sub, tag, width=372):
    """The chapter's words for the column left of the window: the input it uses, a title, a line under it."""
    img = Image.new("RGBA", (width, 420), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    y = 0
    if tag:
        tw = int(F_TAG.getlength(tag.upper())) + 28
        d.rounded_rectangle((0, y, tw, y + 32), 16, fill=ACCENT + (40,), outline=ACCENT + (150,), width=1)
        d.text((14, y + 5), tag.upper(), font=F_TAG, fill=(170, 198, 255))
        y += 54
    for line in wrap(title, F_TITLE, width):
        d.text((0, y), line, font=F_TITLE, fill=TEXT)
        y += 56
    y += 12
    d.rectangle((0, y, 44, y + 3), fill=ACCENT)
    y += 22
    for line in wrap(sub, F_SUB, width):
        d.text((0, y), line, font=F_SUB, fill=MUTED)
        y += 36
    return img.crop((0, 0, width, y + 4))


def key_badge(label):
    keys = [k.strip() for k in label.split("+")]
    pads, gap = 18, 12
    widths = [max(46, int(F_KEY.getlength(k)) + pads * 2) for k in keys]
    plus = int(F_KEY.getlength("+"))
    w = sum(widths) + (len(keys) - 1) * (plus + gap * 2) + 24
    h = 76
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle((0, 0, w - 1, h - 1), 18, fill=(17, 22, 36, 225))
    x = 12
    for i, (k, kw) in enumerate(zip(keys, widths)):
        d.rounded_rectangle((x, 12, x + kw, h - 14), 10, fill=(52, 60, 82, 255))
        d.rounded_rectangle((x, 12, x + kw, h - 18), 10, fill=(70, 80, 106, 255))
        d.text((x + kw / 2, 12 + (h - 30) / 2), k, font=F_KEY, fill=(240, 243, 250), anchor="mm")
        x += kw
        if i < len(keys) - 1:
            d.text((x + gap + plus / 2, h / 2 - 2), "+", font=F_KEY, fill=MUTED, anchor="mm")
            x += plus + gap * 2
    return img


def window_chrome(w, title, scale=1.0):
    """The title bar of a light window: the app's mark, the file's name, and the three buttons."""
    bh = 40
    bar = Image.new("RGBA", (w, bh), (236, 239, 244, 255))
    d = ImageDraw.Draw(bar)
    bar.alpha_composite(logo(20, (60, 66, 80)), (14, 10))
    d.text((44, bh / 2), f"{title} \u2014 InkPDF", font=F_WINDOW, fill=(60, 66, 80), anchor="lm")
    x = w - 30
    c = (90, 96, 110)
    d.line((x - 6, 14, x + 6, 26), fill=c, width=2)
    d.line((x - 6, 26, x + 6, 14), fill=c, width=2)
    x -= 46
    d.rectangle((x - 6, 14, x + 6, 26), outline=c, width=2)
    x -= 46
    d.line((x - 6, 20, x + 6, 20), fill=c, width=2)
    d.line((0, bh - 1, w, bh - 1), fill=(214, 218, 226))
    return bar


# ---- One take
class Take:
    def __init__(self, name):
        self.name = name
        self.dir = os.path.join(TAKES, name)
        with open(os.path.join(self.dir, "take.json")) as f:
            j = json.load(f)
        self.frames = j["frames"]
        self.times = [fr["t"] for fr in self.frames]
        self.events = j["events"]
        self.client = j["client"]
        chapters = [e for e in self.events if e["kind"] == "chapter"]
        self.start = max(0.0, chapters[0]["t"] - 0.25)
        ends = [e["t"] for e in self.events if e["kind"] == "end"]
        self.end = ends[0] if ends else self.frames[-1]["t"]
        self.chapters = chapters
        self.cache = {}
        self.cursor_on = sorted((e["t"], e["show"]) for e in self.events if e["kind"] == "cursor")

    def frame_at(self, t):
        i = max(0, bisect.bisect_right(self.times, t) - 1)
        return self.frames[i]

    def load(self, f):
        a = cv2.imread(os.path.join(self.dir, "frames", f), cv2.IMREAD_COLOR)
        return cv2.cvtColor(a, cv2.COLOR_BGR2RGB)

    def image(self, fr):
        f = fr["file"]
        if f not in self.cache:
            if len(self.cache) > 6:
                self.cache.pop(next(iter(self.cache)))
            self.cache[f] = Image.fromarray(self.load(f))
        return self.cache[f]

    def cursor_visible(self, t):
        on = False
        for at, show in self.cursor_on:
            if at <= t:
                on = show
        return on

    def chapter_at(self, t):
        cur = None
        for c in self.chapters:
            if c["t"] <= t + 1e-6:
                cur = c
        return cur


# ---- The compositor
class Stage:
    def __init__(self):
        self.bg = stage_background()
        self.cursor, self.tip = cursor_sprite()
        self.captions = {}
        self.badges = {}
        self.chrome = {}
        self.masks = {}
        self.shadows = {}
        # The window: 1440 x 900 inside, with a 40-pixel title bar, right of the caption column.
        self.win = (432, (H - 940) // 2 + 40, 1440, 900)

    def caption(self, c):
        key = (c["title"], c["sub"], c.get("tag", ""))
        if key not in self.captions:
            self.captions[key] = caption_card(*key)
        return self.captions[key]

    def badge(self, label):
        if label not in self.badges:
            self.badges[label] = key_badge(label)
        return self.badges[label]

    def mask(self, w, h, r):
        if (w, h, r) not in self.masks:
            self.masks[(w, h, r)] = rounded_mask(w, h, r)
        return self.masks[(w, h, r)]

    def drop(self, w, h, r):
        if (w, h, r) not in self.shadows:
            self.shadows[(w, h, r)] = shadow(w, h, r)
        return self.shadows[(w, h, r)]

    def brand(self, img, alpha=1.0):
        mark = logo(40)
        if alpha < 1:
            mark.putalpha(mark.getchannel("A").point(lambda a: a * alpha))
        img.alpha_composite(mark, (48, 46))
        d = ImageDraw.Draw(img)
        d.text((100, 66), "InkPDF", font=F_BRAND, fill=TEXT + (int(255 * alpha),), anchor="lm")

    # -- a frame of a take at its time t
    def render(self, take, t, state):
        fr = take.frame_at(t)
        shot = take.image(fr)
        img = self.bg.convert("RGBA")
        full = fr["w"] >= W and fr["h"] >= H * 0.95  # (presenting: the screen itself)
        narrow = take.client[0] < 800

        # Where the picture goes: the window, or the whole stage while presenting (it grows and shrinks over 0.45 s).
        x, y, w, h = self.win
        if narrow:
            w, h = take.client
            x, y = self.win[0] + (self.win[2] - w) // 2, (H - h) // 2
        target = (0.0, 0.0, float(W), float(H)) if full else (float(x), float(y), float(w), float(h))
        if state.get("target") != target:
            state["from"] = state.get("rect", target)
            state["target"], state["since"] = target, t
        k = ease((t - state["since"]) / 0.45)
        rect = tuple(a + (b - a) * k for a, b in zip(state["from"], target))
        state["rect"] = rect
        rx, ry, rw, rh = (int(round(v)) for v in rect)
        grown = 1.0 - (rw - w) / max(1.0, W - w) if not narrow else 1.0  # 1 as a window, 0 as the whole stage

        # The caption column (hidden by the window as it grows over it).
        c = take.chapter_at(t)
        if c:
            self.draw_caption(img, take, c, t)
        self.brand(img)

        bar = 0 if narrow else int(round(40 * grown))
        radius = int(round((36 if narrow else 14) * grown))
        if narrow:
            # A phone: a dark body around the screen.
            body = 14
            sh, pad = self.drop(rw + body * 2, rh + body * 2, 44)
            img.alpha_composite(sh, (rx - body - pad, ry - body - pad))
            frame = Image.new("RGBA", (rw + body * 2, rh + body * 2), (24, 27, 36, 255))
            frame.putalpha(self.mask(rw + body * 2, rh + body * 2, 50))
            img.alpha_composite(frame, (rx - body, ry - body))
            ImageDraw.Draw(img).rounded_rectangle((rx - body, ry - body, rx + rw + body - 1, ry + rh + body - 1), 50, outline=(70, 76, 92), width=2)
        elif radius > 0:
            sh, pad = self.drop(rw, rh + bar, radius)
            sh2 = sh if grown > 0.99 else sh.resize(sh.size)
            img.alpha_composite(sh2, (rx - pad, ry - bar - pad))

        pic = shot if shot.size == (rw, rh) else shot.resize((rw, rh), Image.LANCZOS)
        pic = pic.convert("RGBA")
        if bar:
            whole = Image.new("RGBA", (rw, rh + bar))
            chrome_key = (take.name, rw)
            if chrome_key not in self.chrome:
                self.chrome[chrome_key] = window_chrome(rw, TITLES[take.name])
            whole.paste(self.chrome[chrome_key], (0, 0))
            whole.paste(pic, (0, bar))
            pic = whole
        if radius:
            pic.putalpha(self.mask(pic.width, pic.height, radius))
        img.alpha_composite(pic, (rx, ry - bar))

        # What the hands do, mapped from the window's pixels to the stage.
        sx, sy = rw / take.client[0], rh / take.client[1]
        if full:
            sx, sy = rw / (fr["w"] * fr["scale"]), rh / (fr["h"] * fr["scale"])

        def at(px, py):
            return rx + px * sx, ry + py * sy

        if k >= 1 and not full:  # (not while the window grows or shrinks, nor over the slides)
            self.draw_hands(img, take, t, fr, at, (rx, ry, rw, rh))
        self.draw_keys(img, take, t, (rx, ry, rw, rh))
        if full and grown < 0.02 and c:
            self.draw_overlay_caption(img, take, c, t)
        return img.convert("RGB")

    def draw_caption(self, img, take, c, t):
        card = self.caption(c)
        k = ease((t - c["t"]) / 0.45)
        prev = [p for p in take.chapters if p["t"] < c["t"]]
        y0 = (H - card.height) // 2 + 20
        if prev and t - c["t"] < 0.25:
            old = self.caption(prev[-1])
            fade = 1 - (t - c["t"]) / 0.25
            o = old.copy()
            o.putalpha(o.getchannel("A").point(lambda a: a * fade))
            img.alpha_composite(o, (52, (H - old.height) // 2 + 20 - int(10 * (1 - fade))))
            k = 0.0
        if k > 0:
            n = card.copy()
            n.putalpha(n.getchannel("A").point(lambda a: a * k))
            img.alpha_composite(n, (52, y0 + int(14 * (1 - k))))

    def draw_overlay_caption(self, img, take, c, t):
        """While presenting, the chapter's title as a pill at the foot of the screen for a moment."""
        start = max(c["t"], self.full_since(take, t))
        age = t - start
        a = ease(age / 0.4) * (1 - ease((age - 2.8) / 0.5))
        if a <= 0:
            return
        text = c["title"]
        tw = int(F_TITLE.getlength(text)) + 64
        pill = Image.new("RGBA", (tw, 74), (0, 0, 0, 0))
        d = ImageDraw.Draw(pill)
        d.rounded_rectangle((0, 0, tw - 1, 73), 37, fill=(13, 18, 33, 215))
        d.text((tw / 2, 36), text, font=F_TITLE, fill=TEXT, anchor="mm")
        pill.putalpha(pill.getchannel("A").point(lambda v: v * a))
        img.alpha_composite(pill, ((W - tw) // 2, H - 130))

    def full_since(self, take, t):
        since = t
        i = bisect.bisect_right(take.times, t) - 1
        while i > 0 and take.frames[i]["w"] >= W:
            i -= 1
        return take.times[i + 1] if i + 1 < len(take.times) else t

    def draw_hands(self, img, take, t, fr, at, rect):
        rx, ry, rw, rh = rect
        over = Image.new("RGBA", img.size, (0, 0, 0, 0))
        d = ImageDraw.Draw(over)
        # Clicks: a ring that opens and fades.
        for e in take.events:
            if e["kind"] in ("click", "press") and 0 <= t - e["t"] < 0.5:
                u = (t - e["t"]) / 0.5
                cx, cy = at(e["x"], e["y"])
                r = 8 + 26 * ease(u)
                a = int(200 * (1 - u))
                d.ellipse((cx - r, cy - r, cx + r, cy + r), outline=ACCENT + (a,), width=4)
                d.ellipse((cx - 9, cy - 9, cx + 9, cy + 9), fill=ACCENT + (int(a * 0.5),))
        # The stylus tip: a small dark point with a light ring, from its first point to a moment after its last.
        for e in take.events:
            if e["kind"] not in ("pen", "finger"):
                continue
            pts = e["points"]
            t0, t1 = pts[0][2], pts[-1][2]
            linger = 0.18
            if not (t0 - 0.05 <= t <= t1 + linger):
                continue
            j = min(len(pts) - 1, max(0, bisect.bisect_right([p[2] for p in pts], t) - 1))
            px, py = at(pts[j][0], pts[j][1])
            fade = 1.0 if t <= t1 else 1 - (t - t1) / linger
            if e["kind"] == "pen":
                d.ellipse((px - 11, py - 11, px + 11, py + 11), fill=(255, 255, 255, int(120 * fade)))
                d.ellipse((px - 6, py - 6, px + 6, py + 6), fill=(30, 34, 46, int(235 * fade)), outline=(255, 255, 255, int(255 * fade)), width=2)
            else:
                d.ellipse((px - 26, py - 26, px + 26, py + 26), fill=(255, 255, 255, int(70 * fade)), outline=(40, 44, 58, int(110 * fade)), width=2)
        img.alpha_composite(over)
        # The mouse pointer, where the camera saw it, while the mouse is in use and inside the window.
        if take.cursor_visible(t):
            cx, cy = fr["cursor"]
            if fr["scale"] > 1:
                cx, cy = cx * fr["scale"], cy * fr["scale"]
            px, py = at(cx, cy)
            if rx - 4 <= px <= rx + rw and ry - 4 <= py <= ry + rh:
                img.alpha_composite(self.cursor, (int(px - self.tip[0]), int(py - self.tip[1])))

    def draw_keys(self, img, take, t, rect):
        rx, ry, rw, rh = rect
        for e in take.events:
            if e["kind"] == "keys" and 0 <= t - e["t"] < 1.3:
                age = t - e["t"]
                a = ease(age / 0.15) * (1 - ease((age - 1.0) / 0.3))
                b = self.badge(e["label"]).copy()
                b.putalpha(b.getchannel("A").point(lambda v: v * a))
                img.alpha_composite(b, (int(rx + (rw - b.width) / 2), int(ry + rh - b.height - 36 + 8 * (1 - a))))

    # -- the cards
    def title_card(self, t, dur):
        img = self.bg.convert("RGBA")
        d = ImageDraw.Draw(img)
        out = 1 - ease((t - (dur - 0.4)) / 0.4)

        def put(thing, xy, appear):
            a = ease((t - appear) / 0.5) * out
            if a <= 0:
                return
            p = thing.copy()
            p.putalpha(p.getchannel("A").point(lambda v: v * a))
            img.alpha_composite(p, (xy[0], xy[1] + int(12 * (1 - a))))

        put(logo(132), ((W - 132) // 2, 300), 0.1)
        for text, f, colour, y, appear in [("InkPDF", font("segoeuib.ttf", 92), TEXT, 470, 0.3),
                                           ("A fast, pen-first PDF annotator", font("segoeui.ttf", 36), MUTED, 590, 0.55),
                                           ("Windows  \u00b7  Linux  \u00b7  Android", font("seguisb.ttf", 24), (150, 186, 255), 662, 0.8)]:
            tw = int(f.getlength(text))
            layer = Image.new("RGBA", (tw + 20, f.size + 30), (0, 0, 0, 0))
            ImageDraw.Draw(layer).text((10, 5), text, font=f, fill=colour)
            put(layer, ((W - tw) // 2 - 10, y), appear)
        del d
        return img.convert("RGB")

    def end_card(self, t, dur):
        img = self.bg.convert("RGBA")

        def put(thing, xy, appear):
            a = ease((t - appear) / 0.5)
            p = thing.copy()
            p.putalpha(p.getchannel("A").point(lambda v: v * a))
            img.alpha_composite(p, (xy[0], xy[1] + int(12 * (1 - a))))

        put(logo(104), ((W - 104) // 2, 330), 0.0)
        for text, f, colour, y, appear in [("InkPDF", font("segoeuib.ttf", 76), TEXT, 455, 0.15),
                                           ("Open a PDF. Write on it. Save it back.", font("segoeui.ttf", 34), MUTED, 560, 0.35),
                                           ("Free and open source  \u00b7  MIT or Apache-2.0", font("seguisb.ttf", 24), (150, 186, 255), 632, 0.6)]:
            tw = int(f.getlength(text))
            layer = Image.new("RGBA", (tw + 20, f.size + 30), (0, 0, 0, 0))
            ImageDraw.Draw(layer).text((10, 5), text, font=f, fill=colour)
            put(layer, ((W - tw) // 2 - 10, y), appear)
        return img.convert("RGB")


# ---- The film
def film(stage):
    """Every frame of the film, in order, as PIL images."""
    title, end = 3.2, 3.6
    for i in range(int(title * FPS)):
        yield stage.title_card(i / FPS, title)
    fade_n = int(FADE * FPS)
    tail = []
    for name in ORDER:
        take = Take(name)
        state = {}
        n = int((take.end - take.start) * FPS)
        frames = (stage.render(take, take.start + i / FPS, state) for i in range(n))
        head = [next(frames) for _ in range(fade_n)]
        if tail:
            for k, (a, b) in enumerate(zip(tail, head)):
                yield Image.blend(a, b, ease((k + 1) / (fade_n + 1)))
        else:
            for f in head:
                yield f
        buf = []
        for f in frames:
            buf.append(f)
            if len(buf) > fade_n:
                yield buf.pop(0)
        tail = buf
    last = tail[-1] if tail else None
    ends = [stage.end_card(i / FPS, end) for i in range(int(end * FPS))]
    for k, (a, b) in enumerate(zip(tail, ends)):
        yield Image.blend(a, b, ease((k + 1) / (fade_n + 1)))
    for f in ends[len(tail):]:
        yield f
    del last


def ffmpeg():
    try:
        import imageio_ffmpeg

        return imageio_ffmpeg.get_ffmpeg_exe()
    except ImportError:
        return "ffmpeg"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--preview", action="store_true")
    ap.add_argument("--out", default=os.path.join(REPO, "docs"))
    args = ap.parse_args()
    stage = Stage()
    if args.preview:
        folder = os.path.join(TAKES, "preview")
        os.makedirs(folder, exist_ok=True)
        for i, f in enumerate(film(stage)):
            if i % 45 == 0:
                f.save(os.path.join(folder, f"{i:05d}.png"))
        print("preview in", folder)
        return
    mp4 = os.path.join(args.out, "demo.mp4")
    gif = os.path.join(args.out, "demo.gif")
    enc = subprocess.Popen([ffmpeg(), "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS),
                            "-i", "-", "-c:v", "libx264", "-preset", "slow", "-crf", "17", "-tune", "animation", "-pix_fmt", "yuv420p",
                            "-movflags", "+faststart", mp4], stdin=subprocess.PIPE)
    n = 0
    for f in film(stage):
        enc.stdin.write(f.tobytes())
        n += 1
        if n % 300 == 0:
            print(f"  {n / FPS:.0f} s")
    enc.stdin.close()
    enc.wait()
    print(f"{mp4}: {n / FPS:.1f} s, {os.path.getsize(mp4) / 1e6:.1f} MB")
    # The GIF from the film: 960 wide, 15 frames a second, one palette made from the frames that change, no dithering (the stage is
    # smooth enough without it, and the GIF a third smaller).
    palette = os.path.join(TAKES, "palette.png")
    scale = "fps=15,scale=960:-1:flags=lanczos"
    subprocess.run([ffmpeg(), "-y", "-loglevel", "error", "-i", mp4, "-vf", f"{scale},palettegen=max_colors=256:stats_mode=diff", palette],
                   check=True)
    subprocess.run([ffmpeg(), "-y", "-loglevel", "error", "-i", mp4, "-i", palette, "-lavfi",
                    f"{scale}[v];[v][1:v]paletteuse=dither=none:diff_mode=rectangle", "-loop", "0", gif], check=True)
    print(f"{gif}: {os.path.getsize(gif) / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
