"""Records the README tour: drives the real app (the release build, the same synthetic stylus, finger, mouse and keyboard as tools/e2e)
through what it can do while a camera thread films the window at 30 frames a second. Every click, key, pen and finger point is logged
with its time, so compose.py can draw the pointer, touches and key badges in step with the film and cut it into docs/demo.mp4 and
docs/demo.gif.

It takes over the mouse and keyboard for about two minutes: do not touch the machine while it runs. Windows only.

    cargo build --release -p inkpdf
    python tools/demo/tour.py                 every take, then compose.py
    python tools/demo/tour.py --only forms    one take (pen, read, forms, present, narrow), to try it again

Takes go to inkpdf-demo beside the repository (INKPDF_DEMO_DIR to choose); nothing is written inside it.
"""

import argparse
import concurrent.futures
import ctypes
import json
import math
import os
import shutil
import subprocess
import sys
import threading
import time

sys.dont_write_bytecode = True
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "e2e"))

import cv2  # noqa: E402
import numpy as np  # noqa: E402

import documents  # noqa: E402
from core import Report, Session  # noqa: E402
from win import BITMAPINFOHEADER, HANDLE, Windows, gdi32, user32, wt  # noqa: E402

REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
OUT = os.environ.get("INKPDF_DEMO_DIR") or os.path.join(os.path.dirname(REPO), "inkpdf-demo")
FPS = 30
# The window's inside, in screen pixels: 1440 x 900 is shown one to one in a 1920 x 1080 film.
CLIENT = (1440, 900)
NARROW = (480, 960)

gdi32.StretchBlt.argtypes = [HANDLE, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, HANDLE, ctypes.c_int, ctypes.c_int, ctypes.c_int,
                             ctypes.c_int, wt.DWORD]
gdi32.SetStretchBltMode.argtypes = [HANDLE, ctypes.c_int]
user32.MoveWindow.argtypes = [HANDLE, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, wt.BOOL]
user32.GetWindowRect.argtypes = [HANDLE, ctypes.POINTER(wt.RECT)]
user32.GetCursorPos.argtypes = [ctypes.POINTER(wt.POINT)]
now = time.perf_counter


# ---- The camera
class Camera(threading.Thread):
    """Films the window's inside into numbered PNGs (a frame that did not change is not written again) and lists each frame's time,
    file, size and where the mouse pointer was. A window larger than 2000 pixels (presenting, full screen) is filmed at half size."""

    def __init__(self, driver, folder):
        super().__init__(daemon=True)
        self.d, self.folder = driver, folder
        os.makedirs(folder, exist_ok=True)
        self.frames = []
        self.stopping = threading.Event()
        self.pool = concurrent.futures.ThreadPoolExecutor(4)
        self.t0 = None

    def grab(self):
        rect, pt = wt.RECT(), wt.POINT(0, 0)
        user32.GetClientRect(self.d.hwnd, ctypes.byref(rect))
        user32.ClientToScreen(self.d.hwnd, ctypes.byref(pt))
        w, h = rect.right - rect.left, rect.bottom - rect.top
        k = 2 if w > 2000 else 1
        ow, oh = w // k, h // k
        screen = user32.GetDC(None)
        mem = gdi32.CreateCompatibleDC(screen)
        bmp = gdi32.CreateCompatibleBitmap(screen, ow, oh)
        old = gdi32.SelectObject(mem, bmp)
        if k == 1:
            gdi32.BitBlt(mem, 0, 0, w, h, screen, pt.x, pt.y, 0x00CC0020)
        else:
            gdi32.SetStretchBltMode(mem, 4)  # (HALFTONE: averaged, not dropped, pixels)
            gdi32.StretchBlt(mem, 0, 0, ow, oh, screen, pt.x, pt.y, w, h, 0x00CC0020)
        info = BITMAPINFOHEADER(40, ow, -oh, 1, 32, 0, 0, 0, 0, 0, 0)
        buf = np.empty((oh, ow, 4), np.uint8)
        gdi32.GetDIBits(mem, bmp, 0, oh, buf.ctypes.data, ctypes.byref(info), 0)
        gdi32.SelectObject(mem, old)
        gdi32.DeleteObject(bmp)
        gdi32.DeleteDC(mem)
        user32.ReleaseDC(None, screen)
        cur = wt.POINT()
        user32.GetCursorPos(ctypes.byref(cur))
        return buf[:, :, :3], ((cur.x - pt.x) / k, (cur.y - pt.y) / k), k

    def run(self):
        last, last_file, n = None, None, 0
        tick = now()
        while not self.stopping.is_set():
            t = now()
            try:
                img, cursor, k = self.grab()
            except Exception:
                img = None
            if img is not None and img.size:
                if last is None or last.shape != img.shape or not np.array_equal(last, img):
                    n += 1
                    last_file = f"{n:05d}.png"
                    path = os.path.join(self.folder, last_file)
                    self.pool.submit(cv2.imwrite, path, img.copy(), [cv2.IMWRITE_PNG_COMPRESSION, 1])
                    last = img
                self.frames.append({"t": t - self.t0, "file": last_file, "w": img.shape[1], "h": img.shape[0], "scale": k,
                                    "cursor": cursor})
            tick += 1 / FPS
            time.sleep(max(0.0, tick - now()))

    def stop(self):
        self.stopping.set()
        self.join(5)
        self.pool.shutdown(wait=True)


# ---- Hand-drawn strokes
def resample(points, step):
    """Points along a polyline every `step` pixels."""
    out = [points[0]]
    carry = 0.0
    for (x0, y0), (x1, y1) in zip(points, points[1:]):
        seg = math.hypot(x1 - x0, y1 - y0)
        d = step - carry
        while d <= seg:
            t = d / seg
            out.append((x0 + (x1 - x0) * t, y0 + (y1 - y0) * t))
            d += step
        carry = seg - (d - step)
    out.append(points[-1])
    return out


def smooth(points, n=8):
    """A Catmull-Rom curve through `points`, `n` points per span."""
    p = [points[0]] + list(points) + [points[-1]]
    out = []
    for i in range(1, len(p) - 2):
        p0, p1, p2, p3 = p[i - 1], p[i], p[i + 1], p[i + 2]
        for s in range(n):
            t = s / n
            out.append(tuple(0.5 * ((2 * p1[k]) + (-p0[k] + p2[k]) * t + (2 * p0[k] - 5 * p1[k] + 4 * p2[k] - p3[k]) * t * t
                                    + (-p0[k] + 3 * p1[k] - 3 * p2[k] + p3[k]) * t ** 3) for k in (0, 1)))
    out.append(points[-1])
    return out


def ellipse(cx, cy, rx, ry, turns=1.12, start=-2.4, tilt=-0.06, wobble=0.04):
    pts = []
    for i in range(90):
        a = start + turns * math.tau * i / 89
        r = 1 + wobble * math.sin(a * 2.3 + 0.7) + 0.05 * i / 89
        x, y = rx * r * math.cos(a), ry * r * math.sin(a)
        pts.append((cx + x * math.cos(tilt) - y * math.sin(tilt), cy + x * math.sin(tilt) + y * math.cos(tilt)))
    return pts


def pressures(n, base=330, peak=760):
    """Light at the touch, firm in the middle, lifting at the end."""
    out = []
    for i in range(n):
        t = i / max(1, n - 1)
        swell = math.sin(math.pi * min(1.0, t * 1.15)) ** 0.5
        out.append(int(base + (peak - base) * swell + 40 * math.sin(i * 0.31)))
    return out


# ---- The director
class Director:
    """What a take says: the app's input with a person's timing, each action logged for the composer."""

    def __init__(self, folder):
        self.folder = folder
        self.d = Windows()
        self.s = Session(self.d, Report())
        self.events = []
        self.camera = None
        self.mouse_at = None
        self.overhead = None

    # -- the app and the camera
    def launch(self, doc, settings, mode="desktop", size=CLIENT, take="take"):
        d = self.d
        subprocess.run(["taskkill", "/F", "/IM", "inkpdf.exe"], capture_output=True)
        appdata = os.path.join(d.sp, "demo-appdata")
        os.environ["APPDATA"] = appdata
        shutil.rmtree(os.path.join(appdata, "InkPDF"), ignore_errors=True)
        os.makedirs(os.path.join(appdata, "InkPDF"))
        with open(os.path.join(appdata, "InkPDF", "settings.txt"), "w", newline="\n") as f:
            f.write(settings + "author InkPDF\n")
        d.file = os.path.join(DOCS, doc) if doc else None
        d.args = ["--mode", mode]
        d._start()
        self.s.wait_for_layout()
        # The outside of the window is the inside plus its frame; it is placed so the inside has exactly `size` pixels.
        rect, inner = wt.RECT(), wt.RECT()
        user32.GetWindowRect(d.hwnd, ctypes.byref(rect))
        user32.GetClientRect(d.hwnd, ctypes.byref(inner))
        fw, fh = (rect.right - rect.left) - inner.right, (rect.bottom - rect.top) - inner.bottom
        user32.MoveWindow(d.hwnd, 80, 60, size[0] + fw, size[1] + fh, True)
        time.sleep(1.0)
        d._measure()
        self.park()
        self.take = os.path.join(self.folder, take)
        shutil.rmtree(self.take, ignore_errors=True)
        self.events = []
        self.camera = Camera(d, os.path.join(self.take, "frames"))
        self.camera.t0 = now()
        self.camera.start()
        self.wait(400)

    def cut(self, meta=None):
        self.wait(300)
        self.camera.stop()
        with open(os.path.join(self.take, "take.json"), "w") as f:
            json.dump({"frames": self.camera.frames, "events": self.events, "client": self.d.size, **(meta or {})}, f)
        self.d.stop()
        print(f"  {os.path.basename(self.take)}: {len(self.camera.frames)} frames, {self.camera.frames[-1]['t']:.1f} s")

    def log(self, kind, **kw):
        self.events.append({"t": now() - self.camera.t0, "kind": kind, **kw})

    def chapter(self, title, sub="", tag=""):
        """A caption for what follows, and `tag`, the input it uses."""
        self.log("chapter", title=title, sub=sub, tag=tag)

    def wait(self, ms):
        time.sleep(ms / 1000)

    def snap(self, name):
        """A picture of the window now, for whoever writes a take (not part of the film)."""
        os.makedirs(os.path.join(self.folder, "snaps"), exist_ok=True)
        self.s.save_png(self.s.shot(), os.path.join(self.folder, "snaps", name + ".png"))

    def find(self, rgb, tol=30, box=(0, 0, 99999, 99999)):
        """The box (x0, y0, x1, y1) of the pixels of this colour in the window now, to place a stroke on what the page shows."""
        img = self.s.shot()
        a = np.frombuffer(img.data, np.uint8).reshape(img.h, img.w, 4)[:, :, list(img.order)].astype(int)
        hit = np.zeros(a.shape[:2], bool)
        bx0, by0, bx1, by1 = box
        hit[by0:by1, bx0:bx1] = (np.abs(a[by0:by1, bx0:bx1] - np.array(rgb)).sum(axis=2) < tol)
        ys, xs = np.nonzero(hit)
        if not len(xs):
            raise RuntimeError(f"no {rgb} in the window")
        return int(xs.min()), int(ys.min()), int(xs.max()), int(ys.max())

    def spot(self, group, name):
        return self.s.spot(group, name)

    # -- the mouse: it glides to a place and presses there, as a hand would
    def park(self):
        """The pointer out of the way (below the window) until the mouse is used."""
        ox, oy = self.d._origin
        user32.SetCursorPos(ox + self.d.size[0] // 2, oy + self.d.size[1] + 200)
        self.mouse_at = None

    def glide(self, p, seconds=None):
        ox, oy = self.d._origin
        if self.mouse_at is None:
            # The pointer comes in from below the window.
            self.mouse_at = (p[0] + 120, self.d.size[1] + 40)
            self.log("cursor", show=True)
        x0, y0 = self.mouse_at
        dist = math.hypot(p[0] - x0, p[1] - y0)
        seconds = seconds or min(0.75, 0.28 + dist / 1800)
        steps = max(2, int(seconds * 120))
        for i in range(1, steps + 1):
            t = i / steps
            e = t * t * (3 - 2 * t)
            arc = math.sin(math.pi * t) * min(40, dist * 0.06)  # (a hand moves along a slight curve)
            user32.SetCursorPos(int(ox + x0 + (p[0] - x0) * e), int(oy + y0 + (p[1] - y0) * e - arc))
            time.sleep(seconds / steps)
        self.mouse_at = p

    def click(self, p, hold=0.0):
        if isinstance(p, str):
            p = self.spot("buttons", p)
        self.glide(p)
        self.wait(120)
        self.log("click", x=p[0], y=p[1])
        user32.mouse_event(0x2, 0, 0, 0, 0)
        time.sleep(0.07 + hold)
        user32.mouse_event(0x4, 0, 0, 0, 0)
        self.wait(250)

    def bar(self, name):
        self.click(self.spot("bar", name))

    def drag(self, p0, p1, seconds=0.8):
        ox, oy = self.d._origin
        self.glide(p0)
        self.wait(100)
        self.log("press", x=p0[0], y=p0[1])
        user32.mouse_event(0x2, 0, 0, 0, 0)
        self.wait(80)
        steps = int(seconds * 120)
        for i in range(1, steps + 1):
            t = i / steps
            e = t * t * (3 - 2 * t)
            user32.SetCursorPos(int(ox + p0[0] + (p1[0] - p0[0]) * e), int(oy + p0[1] + (p1[1] - p0[1]) * e))
            time.sleep(seconds / steps)
        self.wait(60)
        user32.mouse_event(0x4, 0, 0, 0, 0)
        self.mouse_at = p1
        self.log("release", x=p1[0], y=p1[1])
        self.wait(200)

    def wheel(self, p, notches):
        """The mouse wheel turned `notches` (down is negative) over `p`."""
        self.glide(p)
        for _ in range(abs(notches)):
            user32.mouse_event(0x0800, 0, 0, 120 if notches > 0 else -120 & 0xFFFFFFFF, 0)
            time.sleep(0.07)
        self.log("wheel", x=p[0], y=p[1])
        self.wait(300)

    def hide_mouse(self):
        if self.mouse_at is not None:
            self.log("cursor", show=False)
        self.park()

    # -- keys, with a badge on the film for a shortcut
    def keys(self, spec, badge=None):
        if badge is not False:
            names = {"ctrl": "Ctrl", "esc": "Esc", "right": "→", "enter": "Enter"}
            self.log("keys", label=badge or " + ".join(names.get(k, k.upper()) for k in spec.split("+")))
        self.d.keys(spec)

    def type(self, text, cps=14):
        for ch in text:
            self.d.type_text(ch)  # (it waits 0.15 s after each)
            time.sleep(max(0.0, 1 / cps - 0.15))

    # -- the stylus and the finger
    def bot(self, *ops):
        title = self.d.title_part
        if self.overhead is None:
            t = now()
            subprocess.run([self.d.bot, title, "size"], capture_output=True)
            self.overhead = now() - t
        t = now()
        subprocess.run([self.d.bot, title, *map(str, ops)], capture_output=True)
        return t - self.camera.t0, now() - self.camera.t0

    def pen(self, points, press=None, step=3.0):
        """A stroke through `points` (window pixels), smoothed and spaced like a hand's samples at 250 Hz."""
        self.hide_mouse()
        pts = resample(points, step) if len(points) > 2 else points
        w, h = self.d.size
        press = press or pressures(len(pts))
        ops = ["ppath"]
        for (x, y), p in zip(pts, press):
            ops += [f"{x / w:.5f}", f"{y / h:.5f}", p]
        t0, t1 = self.bot(*ops)
        # The bot starts (as long as a `size` takes), hovers 30 ms, then moves one point every 4 ms; its end is known exactly.
        start, end = t0 + self.overhead + 0.03, t1 - 0.025
        n = len(pts)
        self.log("pen", points=[[x, y, start + (end - start) * i / max(1, n - 1)] for i, (x, y) in enumerate(pts)])
        self.wait(120)

    def pen_tap(self, p):
        if isinstance(p, str):
            p = self.spot("buttons", p)
        self.hide_mouse()
        t0, t1 = self.bot("tap", f"{p[0] / self.d.size[0]:.5f}", f"{p[1] / self.d.size[1]:.5f}")
        self.log("pen", points=[[p[0], p[1], t1 - 0.06], [p[0], p[1], t1 - 0.02]], tap=True)
        self.wait(250)

    def pen_bar(self, name):
        self.pen_tap(self.spot("bar", name))

    def finger(self, points, seconds=0.6, hold=True):
        """One finger along `points`; it rests at the end before it lifts (`hold`), so a pan stops where it is put, without a fling."""
        self.hide_mouse()
        w, h = self.d.size
        pts = resample(points, max(4.0, math.hypot(points[-1][0] - points[0][0], points[-1][1] - points[0][1]) / (seconds * 120)))
        if hold:
            pts += [pts[-1]] * 25
        ops = ["touchpath", 1]
        for x, y in pts:
            ops += [f"{x / w:.5f}", f"{y / h:.5f}"]
        t0, t1 = self.bot(*ops)
        start, end = t0 + self.overhead, t1 - 0.01
        self.log("finger", points=[[x, y, start + (end - start) * i / max(1, len(pts) - 1)] for i, (x, y) in enumerate(pts)])
        self.wait(150)

    def pinch(self, c, r0, r1, seconds=0.6):
        """Two fingers either side of `c`, from `r0` to `r1` pixels apart from it (apart: zoom in), resting before they lift."""
        self.hide_mouse()
        w, h = self.d.size
        steps = max(8, int(seconds * 110))
        radii = [r0 + (r1 - r0) * (i / steps) ** 0.8 for i in range(steps + 1)] + [r1] * 25
        ops = ["touchpaths", 2, len(radii)]
        for r in radii:
            ops += [f"{(c[0] - r) / w:.5f}", f"{c[1] / h:.5f}", f"{(c[0] + r) / w:.5f}", f"{c[1] / h:.5f}"]
        t0, t1 = self.bot(*ops)
        start, end = t0 + self.overhead, t1 - 0.01
        n = len(radii)
        for sign in (-1, 1):
            self.log("finger", points=[[c[0] + sign * r, c[1], start + (end - start) * i / (n - 1)] for i, r in enumerate(radii)])
        self.wait(150)

    def finger_tap(self, p):
        if isinstance(p, str):
            p = self.spot("buttons", p)
        self.hide_mouse()
        w, h = self.d.size
        t0, t1 = self.bot("finger", "down", 1, f"{p[0] / w:.5f}", f"{p[1] / h:.5f}", "finger", "up", 1, f"{p[0] / w:.5f}", f"{p[1] / h:.5f}")
        self.log("finger", points=[[p[0], p[1], t1 - 0.12], [p[0], p[1], t1]], tap=True)
        self.wait(250)


# ---- The takes
PAPER, DECK, FORM = "Writing on PDFs with a Pen.pdf", "Field Study.pdf", "Registration.pdf"
DOCS = os.path.join(OUT, "documents")


def take_paper(a):
    """A paper, written on with the stylus, then read with the mouse and keyboard."""
    a.launch(PAPER, "appearance light\ntool pen\nslot 2\nwidth 2.20\nshape rectangle\nshape-slot 4\nshape-width 2.00\ntext-slot 4\n"
             "text-size 13.00\n", take="1-paper")
    a.chapter("Write naturally with a stylus", "Pressure-sensitive ink that keeps up with the pen", "Stylus")
    a.wait(700)
    # A red ring around "Pen" in the title, a blue line under a phrase, a green tick by the abstract.
    a.pen(smooth(ellipse(972, 224, 82, 44), 2))
    a.wait(350)
    a.pen_bar("swatch1")
    a.pen(smooth([(528, 727), (610, 729), (700, 726), (775, 728), (834, 725)], 6))
    a.wait(300)
    a.pen_bar("swatch3")
    a.pen(smooth([(96, 404), (112, 424), (122, 436), (140, 404), (168, 366)], 5))
    a.wait(900)

    a.chapter("Erase, and undo anything", "The eraser takes whole strokes; every step can be undone", "Stylus")
    a.pen_tap("eraser")
    a.pen(smooth([(80, 380), (130, 420), (180, 440)], 6))
    a.wait(700)
    a.pen_tap("undo")
    a.wait(1000)

    a.chapter("Shapes and typed text", "Saved as standard PDF annotations that any reader shows", "Stylus, mouse and keyboard")
    a.finger([(760, 760), (760, 560), (760, 330)], seconds=0.7)
    a.wait(500)
    x0, y0, x1, y1 = a.find((64, 158, 115), box=(250, 150, 1300, 900))  # the green bar of the figure
    a.pen_tap("geometry")
    a.pen([(x0 - 125, y0 - 14), (x1 + 100, y1 + 16)])
    a.wait(500)
    # (Text is typed: the box is put down with the mouse, by the keyboard's hand.)
    a.click("text")
    a.click((x1 + 128, (y0 + y1) / 2 - 12))
    a.wait(200)
    a.type("the fastest step")
    a.keys("esc", badge=False)
    a.wait(900)
    a.snap("shapes")
    print("   figure bar", x0, y0, x1, y1)

    a.chapter("Select and recolour", "The lasso picks up ink, shapes and text", "Stylus")
    a.hide_mouse()
    a.pen_tap("lasso")
    a.pen(smooth(ellipse((x0 + x1) / 2 + 150, (y0 + y1) / 2, 360, 44, turns=1.02, wobble=0.02), 2))
    a.wait(500)
    a.snap("lassoed")
    a.pen_bar("swatch2")
    a.wait(700)
    a.pen_tap((1000, 760))
    a.wait(800)

    a.chapter("Read and mark the text", "The mouse and keyboard do everything the pen does", "Mouse")
    a.click("select")
    line = y0 - 301  # the line "This note describes ..." above the figure
    a.drag((191, line), (838, line), seconds=1.0)
    a.wait(300)
    a.snap("selected")
    a.bar("highlight")
    a.wait(300)
    a.click((1000, 780))
    a.wait(600)

    a.chapter("Search the whole document", "Every match is lit and listed", "Keyboard")
    a.keys("ctrl+f")
    a.wait(300)
    a.type("latency")
    a.wait(900)
    a.snap("search")
    a.keys("enter", badge="Enter")
    a.wait(900)
    a.keys("esc")
    a.wait(500)

    a.chapter("Every page at a glance", "The pages overview, with the table of contents", "Mouse")
    a.click("pages")
    a.wait(1300)
    a.snap("overview")
    a.click(a.spot("overview/toc", "4 One Input Is Enough"))
    a.wait(1200)
    a.snap("toc")

    a.chapter("Light or dark", "Dark pages too, and the file itself never changes", "Mouse")
    a.click("theme")
    a.wait(1000)
    a.snap("dark")
    tx, ty = a.spot("buttons", "theme")
    a.click((tx, ty + 82))  # the round button under it: dark pages
    a.wait(1400)
    a.snap("dark-pages")
    a.click((tx, ty + 82))
    a.wait(400)
    a.click("theme")
    a.click("theme")
    a.wait(800)
    a.snap("light-again")
    print("   buttons:", sorted(a.d.layout().get("buttons", {})))


def take_form(a):
    """A registration form, filled in with the mouse and keyboard."""
    a.launch(FORM, "appearance light\ntool select\n", take="2-form")
    a.chapter("Fill in PDF forms", "Text fields, choices, radio buttons and check boxes", "Mouse and keyboard")
    a.wait(800)
    a.click(a.spot("fields", "name"))
    a.type("Ada Lovelace")
    a.click(a.spot("fields", "email"))
    a.type("ada@example.org")
    a.click(a.spot("fields", "session#1"))
    a.wait(300)
    a.click(a.spot("fields", "track"))  # (a click picks the next choice)
    a.wait(500)
    a.wheel((720, 600), -6)
    a.wait(500)
    a.click(a.spot("fields", "agree"))
    a.wait(500)
    a.click((1100, 700))
    a.wait(900)


def take_present(a):
    """The slides, full screen."""
    a.launch(DECK, "appearance light\ntool select\n", take="3-present")
    a.chapter("Present with one click", "A whole page at a time, turned by a click, a tap, a swipe or the keys", "Mouse and keyboard")
    a.wait(700)
    a.click("present")
    a.hide_mouse()
    a.wait(1800)
    a.keys("right")
    a.wait(1900)
    a.keys("right")
    a.wait(1900)
    a.keys("right")
    a.wait(1700)
    a.log("end")  # (the film leaves on the last slide; the way back to the window is not in it)
    a.keys("esc")
    a.wait(900)


def take_narrow(a):
    """A narrow screen in tablet mode: the stylus and fingers only."""
    a.launch(PAPER, "appearance light\ntool select\nslot 2\nwidth 2.00\n", mode="tablet", size=NARROW, take="4-narrow")
    a.chapter("At home on small screens", "The toolbar folds into round buttons that open when needed", "Stylus and fingers")
    a.wait(900)
    a.pen_tap("fold-tools")
    a.wait(900)
    a.pen_tap("pen")
    a.wait(600)
    a.snap("narrow-pen")
    x0, y0, x1, y1 = a.find((64, 158, 115), box=(0, 120, 480, 960))
    cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
    a.pinch((cx, cy), 30, 110, seconds=0.7)
    a.wait(700)
    x0, y0, x1, y1 = a.find((64, 158, 115), box=(0, 120, 480, 960))
    a.snap("narrow-zoomed")
    a.pen(smooth(ellipse((x0 + x1) / 2, (y0 + y1) / 2, (x1 - x0) / 2 + 30, (y1 - y0) / 2 + 22), 2))
    a.wait(800)
    a.pen_tap("fold-view")
    a.wait(900)
    a.pen_tap("theme")
    a.wait(1200)
    a.finger_tap((240, 600))
    a.wait(1000)
    a.snap("narrow-end")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--only", nargs="*")
    args = ap.parse_args()
    os.makedirs(DOCS, exist_ok=True)
    documents.paper(os.path.join(DOCS, PAPER))
    documents.deck(os.path.join(DOCS, DECK))
    documents.form(os.path.join(DOCS, FORM))
    a = Director(OUT)
    takes = {"paper": take_paper, "form": take_form, "present": take_present, "narrow": take_narrow}
    for name in args.only or list(takes):
        print("take", name)
        takes[name](a)
        a.cut()


if __name__ == "__main__":
    main()
