"""The parts of the real-input checks that are the same on every platform: the pictures taken of the window and the pixels counted in them,
the shapes a stylus or finger is moved along, and `Session`, which is what the scenarios talk to. A platform's driver (win.py, linux.py,
android.py) only knows how to start the app and send one kind of input; it never decides what a check expects.

Buttons are pressed by NAME: the app is started with a file it keeps saying where every button, bar action, overview page and contents entry
is (apps/inkpdf/src/app/layout.rs), so no position is remembered here and any window size works. Positions in the scenarios are fractions of
the window.
"""

import json
import math
import os
import sys
import time

TAU = math.tau


def lerp(a, b, t):
    return a + (b - a) * t


# ---- Shapes: lists of (fx, fy), fractions of the window. The same ones for every platform's stylus and finger.
def line_path(x0, y0, x1, y1, steps):
    steps = max(int(steps), 2)
    return [(lerp(x0, x1, s / steps), lerp(y0, y1, s / steps)) for s in range(steps + 1)]


def stroke_path(x0, y0, x1, y1, amp, cycles, steps):
    steps = max(int(steps), 2)
    out = []
    for s in range(steps + 1):
        t = s / steps
        out.append((lerp(x0, x1, t), lerp(y0, y1, t) + amp * math.sin(t * cycles * TAU)))
    return out


def pinch_paths(cx, cy, r0, r1, steps):
    """The two fingers of a pinch, from `r0` to `r1` either side of (cx, cy)."""
    steps = max(int(steps), 2)
    radii = [lerp(r0, r1, s / steps) for s in range(steps + 1)]
    return [(cx - r, cy) for r in radii], [(cx + r, cy) for r in radii]


class Image:
    """A screenshot of the window as raw pixels, `bpp` bytes each, with the position of red, green and blue in a pixel."""

    def __init__(self, w, h, data, order=(0, 1, 2), bpp=4):
        self.w, self.h, self.data, self.order, self.bpp = w, h, data, order, bpp

    def count(self, rgb, x0, y0, x1, y1, tol=45):
        """Pixels within `tol` of `rgb` in a rectangle, in the picture's own pixels (the same test on every platform)."""
        ax, ay, bx, by = max(0, int(x0)), max(0, int(y0)), min(self.w, int(x1)), min(self.h, int(y1))
        if bx <= ax or by <= ay:
            return 0
        planes = []
        for want, channel in zip(rgb, self.order):
            table = bytes(1 if abs(v - want) <= tol else 0 for v in range(256))
            rows = []
            for y in range(ay, by):
                start = (y * self.w + ax) * self.bpp + channel
                rows.append(self.data[start : start + (bx - ax) * self.bpp : self.bpp])
            planes.append(b"".join(rows).translate(table))
        # (Three masks of 0 and 1 a byte, and-ed as whole numbers: counting is a C loop, not a Python one.)
        n = len(planes[0])
        both = int.from_bytes(planes[0], "little") & int.from_bytes(planes[1], "little") & int.from_bytes(planes[2], "little")
        return both.to_bytes(n, "little").count(1)


class Skip(Exception):
    """A check this platform cannot do, said so, never passed."""


class Driver:
    """What a platform gives the checks. Positions are window pixels; `fx`, `fy` are fractions of the window."""

    name = "?"
    #: The mode the program starts in when it is not told one: the mouse draws in `desktop` only.
    default_mode = "desktop"

    # -- the app
    def launch(self, settings, pdf, mode):
        raise NotImplementedError

    def relaunch(self):
        raise NotImplementedError

    def stop(self):
        raise NotImplementedError

    def alive(self):
        raise NotImplementedError

    def errors(self):
        raise NotImplementedError

    def layout(self):
        """The file the app keeps, read now: a dict, or None if there is none (yet)."""
        raise NotImplementedError

    @property
    def size(self):
        raise NotImplementedError

    # -- input
    def mouse(self, x0, y0, x1, y1):
        raise NotImplementedError

    def pen(self, path):
        """The stylus tip down along `path` (fractions) and lifted: a tap is two equal points."""
        raise NotImplementedError

    def finger(self, paths):
        """One or more fingers down together, each along its own path (fractions), and lifted."""
        raise NotImplementedError

    def keys(self, spec):
        """A key or chord: "ctrl+z", "esc", "del", "enter", "pgdn", "end", "home", "ctrl+0"."""
        raise NotImplementedError

    def type_text(self, text):
        raise NotImplementedError

    def activate(self, p):
        """Press a control that works in every mode (a folded pill's round button): the mouse, where a mouse is how it is done."""
        self.mouse(p[0], p[1], p[0], p[1])

    def posture(self, name):
        """The system telling the app its posture changed. False when this platform has no such thing."""
        return False

    # -- looking
    def screenshot(self):
        raise NotImplementedError


# ---- Reporting
class Report:
    def __init__(self):
        self.ok = self.failed = self.skipped = 0
        self.lines = []

    def say(self, line):
        self.lines.append(line)
        print(line, flush=True)

    def check(self, cond, message):
        if cond:
            self.ok += 1
            self.say(f"ok:   {message}")
        else:
            self.failed += 1
            self.say(f"FAIL: {message}")

    def skip(self, message, why):
        self.skipped += 1
        self.say(f"SKIP: {message} ({why})")


class Session:
    """What a scenario uses: the driver's input and a few readings, by name. One per run."""

    BLACK, RED, ACCENT = (28, 30, 36), (224, 52, 52), (64, 132, 255)

    def __init__(self, driver, report):
        self.d = driver
        self.report = report
        self.shots = os.environ.get("INKPDF_E2E_SHOTS")
        #: Whether the toolbar is folded into round buttons (a narrow window): seen when the app starts.
        self.narrow = False

    # ---- the app
    def launch(self, settings="", pdf="", mode=None):
        self.d.launch(settings, pdf, mode)
        self.wait_for_layout()

    def reopen(self):
        self.d.relaunch()
        self.wait_for_layout()

    def stop(self):
        self.d.stop()

    def wait_for_layout(self, seconds=15):
        end = time.time() + seconds
        while time.time() < end:
            lay = self.d.layout()
            if lay and "state" in lay:
                self.narrow = any(k.startswith("fold") for k in lay.get("buttons", {}))
                return
            time.sleep(0.15)
        raise RuntimeError("the app never said where its buttons are")

    def state(self):
        lay = self.d.layout() or {}
        return lay.get("state", {})

    def state_of(self, group):
        """One part of what the app says of itself (`presentation`: the page shown and, while it shows, where the button that leaves is)."""
        return (self.d.layout() or {}).get(group, {})

    def pages(self):
        """(the page in view, how many pages), as the window's title says."""
        st = self.state()
        return st.get("page", 0), st.get("pages", 0)

    def dirty(self):
        return bool(self.state().get("dirty"))

    def mode(self):
        return self.state().get("mode")

    def healthy(self):
        return self.d.alive() and not self.d.errors()

    def wait(self, ms):
        time.sleep(ms / 1000)

    # ---- where things are
    def _look_up(self, group, name):
        o = self.d.layout()
        for g in group.split("/"):
            o = o.get(g) if isinstance(o, dict) else None
        return o[name] if isinstance(o, dict) and name in o else None

    def spot(self, group, name=None):
        """The place of a control, in window pixels: `spot("buttons", "undo")`, `spot("overview/pages", "3")`, `spot("fields", "name")`.
        On a narrow window the toolbar is a row of tools and round buttons that open a pill of buttons in its place, one at a time: a
        pill is opened, and put away by a touch on the page, as a person would, until the button shows."""
        found = self._poll(group, name, 1.0)
        if found is not None or group != "buttons" or not self.narrow:
            if found is None:
                raise KeyError(f"the app's layout has no '{name}' in {group}")
            return found
        tried = set()
        for _ in range(6):
            buttons = self.d.layout().get("buttons", {})
            folds = [f for f in ("fold-doc", "fold-tools", "fold-view") if f in buttons and f not in tried]
            if folds:
                tried.add(folds[0])
                self.d.activate(buttons[folds[0]])
            else:
                w, h = self.d.size
                self.d.activate((w * 0.5, h * 0.6))
            found = self._poll(group, name, 1.0)
            if found is not None:
                return found
        raise KeyError(f"the app's layout has no '{name}' in {group}")

    def link(self, kind):
        """The place of a link of this kind (`page3`, `url`) that is in the middle of the window. A window shows as much of a page as its shape
        allows, so the test document has links all down its first page, and the one that is clear of the toolbars and of the keyboard is taken."""
        w, h = self.d.size
        spots = {}
        end = time.time() + 2.0
        while time.time() < end:
            spots = (self.d.layout() or {}).get("links", {})
            ok = [p for n, p in spots.items() if (n == kind or n.startswith(kind + "#")) and 0.25 * h <= p[1] <= 0.7 * h]
            if ok:
                return ok[0]
            time.sleep(0.1)
        raise KeyError(f"no link to {kind} in the middle of the window: {spots}")

    def toolbar_up(self):
        """Whether the app says there are buttons to press at all (a folded toolbar has round buttons, not every one of them)."""
        return bool((self.d.layout() or {}).get("buttons"))

    def has(self, group, name):
        """Whether the app says there is such a control now."""
        return self._look_up(group, name) is not None

    def _poll(self, group, name, seconds):
        end = time.time() + seconds
        while True:
            found = self._look_up(group, name)
            if found is not None or time.time() >= end:
                return found
            time.sleep(0.05)
    # ---- input
    def _frac(self, p):
        w, h = self.d.size
        return p[0] / w, p[1] / h

    def click_spot(self, p):
        self.d.mouse(p[0], p[1], p[0], p[1])

    def mouse_drag(self, fx0, fy0, fx1, fy1):
        w, h = self.d.size
        self.d.mouse(fx0 * w, fy0 * h, fx1 * w, fy1 * h)

    def mouse_px(self, x0, y0, x1, y1):
        self.d.mouse(x0, y0, x1, y1)

    def click(self, fx, fy):
        self.mouse_drag(fx, fy, fx, fy)

    def tap_spot(self, p):
        f = self._frac(p)
        self.d.pen([f, f])

    def pen_tap(self, fx, fy):
        self.d.pen([(fx, fy), (fx, fy)])

    def pen_stroke(self, x0, y0, x1, y1, amp, cycles, steps):
        self.d.pen(stroke_path(x0, y0, x1, y1, amp, cycles, steps))

    def pen_line(self, x0, y0, x1, y1, steps):
        self.d.pen(line_path(x0, y0, x1, y1, steps))

    def finger_drag(self, x0, y0, x1, y1, steps):
        self.d.finger([line_path(x0, y0, x1, y1, steps)])

    def finger_pinch(self, cx, cy, r0, r1, steps):
        self.d.finger(list(pinch_paths(cx, cy, r0, r1, steps)))

    def finger_tap(self, p):
        f = self._frac(p)
        self.d.finger([[f, f]])

    def keys(self, spec):
        self.d.keys(spec)

    def type_text(self, text):
        self.d.type_text(text)

    # Toolbar buttons and tools by name (undo, redo, save, del, plus, pages, search, theme, mode, full; select, pen, geometry, text, lasso,
    # eraser, laser) with the mouse, the stylus tip or a finger; the bar under the tools likewise (delete, copy, duplicate, paste,
    # swatch0..swatch4, style, highlight, ...).
    def press(self, name):
        self.click_spot(self.spot("buttons", name))

    def pen_press(self, name):
        self.tap_spot(self.spot("buttons", name))

    def finger_press(self, name):
        self.finger_tap(self.spot("buttons", name))

    def bar(self, name):
        self.click_spot(self.spot("bar", name))

    def pen_bar(self, name):
        self.tap_spot(self.spot("bar", name))

    def finger_bar(self, name):
        self.finger_tap(self.spot("bar", name))

    def tell_setting(self, name):
        """The system telling the app a setting changed (a posture); False where the platform has no such thing."""
        return self.d.posture(name)

    # ---- looking
    def shot(self, name="look"):
        image = self.d.screenshot()
        if self.shots:
            self.save_png(image, os.path.join(self.shots, f"{name}.png"))
        return image

    @staticmethod
    def save_png(image, path):
        import struct
        import zlib

        os.makedirs(os.path.dirname(path), exist_ok=True)
        r, g, b = image.order
        rows = []
        for y in range(image.h):
            row = image.data[y * image.w * image.bpp : (y + 1) * image.w * image.bpp]
            rgb = bytearray(image.w * 3)
            rgb[0::3], rgb[1::3], rgb[2::3] = row[r :: image.bpp], row[g :: image.bpp], row[b :: image.bpp]
            rows.append(b"\x00" + bytes(rgb))

        def chunk(kind, body):
            c = struct.pack(">I", len(body)) + kind + body
            return c + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)

        png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", image.w, image.h, 8, 2, 0, 0, 0))
        png += chunk(b"IDAT", zlib.compress(b"".join(rows), 3)) + chunk(b"IEND", b"")
        with open(path, "wb") as f:
            f.write(png)

    def count(self, r, g, b, x0=-1, y0=-1, x1=-1, y1=-1, tol=45):
        """Pixels of a colour in a fresh screenshot, in window pixels: the page area, under the toolbar, by default."""
        w, h = self.d.size
        if x0 < 0:
            x0, y0, x1, y1 = 0.15 * w, 0.14 * h, 0.95 * w, 0.93 * h
        return self.shot().count((r, g, b), x0, y0, x1, y1, tol)

    def count_near(self, rgb, p, hw=18, hh=None, tol=45):
        """The same colour within `hw` by `hh` pixels of a spot (a button, a page's picture)."""
        hh = hw if hh is None else hh
        return self.count(*rgb, p[0] - hw, p[1] - hh, p[0] + hw, p[1] + hh, tol)

    def black(self):
        return self.count(*self.BLACK)  # the pen's colour slot 0

    def red(self):
        # (The ink's own red, closely: a page can have reds of its own, and what a window shows of it depends on the window.)
        return self.count(*self.RED, tol=20)  # colour slot 2

    def accent(self):
        return self.count(*self.ACCENT)  # the interface's blue: a selection's frame, a shape's anchors

    # ---- reporting
    def check(self, cond, message):
        self.report.check(cond, message)

    def skip(self, message, why):
        self.report.skip(message, why)


def log(*a):
    print(*a, file=sys.stderr, flush=True)
