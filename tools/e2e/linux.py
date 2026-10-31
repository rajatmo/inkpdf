"""Linux, on a virtual X display (run it under `xvfb-run`, as run.ps1 does): the real mouse and keyboard through XTEST (`xdotool`), screenshots with
ImageMagick's `import`. A virtual display has no stylus or finger to send, so those two are handed to the app's `--inject` file: the same pointer
events a real pen or touch screen gives, as the program's own input vocabulary (input/record.rs), given to the editor as they are written. They
skip the system's input stack but go through the real window, the real editor and the real saving; every other check is the same as everywhere.

Needs: xvfb (xvfb-run), xdotool, imagemagick (import), mesa-vulkan-drivers (lavapipe), libxkbcommon-x11-0 and a system font (fonts-liberation).
The build under test is read from INKPDF_TARGET, or ~/inkpdf-build-linux/release (`cargo build --release -p inkpdf --bins` there)."""

import json
import os
import re
import shutil
import subprocess
import time

from core import Driver, Image

KEYS = {"ctrl": "ctrl", "shift": "shift", "alt": "alt", "esc": "Escape", "del": "Delete", "enter": "Return", "backspace": "BackSpace",
        "pgup": "Prior", "pgdn": "Next", "end": "End", "home": "Home", "=": "equal", "-": "minus", "left": "Left", "up": "Up", "right": "Right", "down": "Down"}
# (Milliseconds between one injected sample and the next: a stylus reports every few milliseconds, and the app looks for input every eight.)
PACE = 0.012


class Linux(Driver):
    name = "linux"
    default_mode = "desktop"

    def __init__(self):
        home = os.path.expanduser("~")
        target = os.environ.get("INKPDF_TARGET") or os.path.join(os.environ.get("CARGO_TARGET_DIR") or os.path.join(home, "inkpdf-build-linux"), "release")
        self.exe, self.gen = os.path.join(target, "inkpdf"), os.path.join(target, "inkpdf-gen")
        self.sp = os.environ.get("INKPDF_E2E_DIR") or os.path.join(home, "inkpdf-e2e-linux")
        os.makedirs(self.sp, exist_ok=True)
        self.layout_file = os.path.join(self.sp, "layout.json")
        self.inject_file = os.path.join(self.sp, "inject.txt")
        self.proc = None
        self.wid = None
        self.file = None
        self.args = []
        self.origin = (0, 0)
        self.env = {}

    # ---- the app
    def launch(self, settings, pdf, mode):
        self.stop()
        fake_home = os.path.join(self.sp, "home")
        shutil.rmtree(fake_home, ignore_errors=True)
        os.makedirs(os.path.join(fake_home, ".config", "InkPDF"))
        with open(os.path.join(fake_home, ".config", "InkPDF", "settings.txt"), "w") as f:
            f.write(settings + "appearance light\n")
        self.env = {k: v for k, v in os.environ.items() if k not in ("WAYLAND_DISPLAY", "XDG_CONFIG_HOME")}
        self.env["HOME"] = fake_home
        self.file = None
        if pdf:
            self.file = os.path.join(self.sp, f"work-{pdf}.pdf")
            self.make_pdf(pdf, self.file)
        self.args = ["--mode", mode] if mode else []
        self._start()

    def make_pdf(self, kind, out):
        if m := re.fullmatch(r"pages(\d+)", kind):
            args = [m.group(1)]
        elif m := re.fullmatch(r"outline(\d+)", kind):
            args = [m.group(1), "--outline"]
        elif m := re.fullmatch(r"links(\d+)", kind):
            args = [m.group(1), "--links"]
        elif kind == "form":
            args = ["1", "--form"]
        else:
            raise ValueError(kind)
        subprocess.run([self.gen, out, *args], capture_output=True, check=True)

    def relaunch(self):
        self.stop()
        self._start()

    def _start(self):
        for path in (self.layout_file, self.inject_file):
            if os.path.exists(path):
                os.remove(path)
        with open(self.inject_file, "w") as f:
            f.write("inkpdf-input 1\n")
        cmd = [self.exe] + ([self.file] if self.file else []) + ["--layout", self.layout_file, "--inject", self.inject_file] + self.args
        self.err_path = os.path.join(self.sp, "app.err.txt")
        self.proc = subprocess.Popen(cmd, env=self.env, stdout=open(os.path.join(self.sp, "app.out.txt"), "wb"), stderr=open(self.err_path, "wb"))
        self.wid = None
        for _ in range(100):
            time.sleep(0.15)
            out = subprocess.run(["xdotool", "search", "--pid", str(self.proc.pid)], capture_output=True, text=True).stdout.split()
            if out and os.path.exists(self.layout_file):
                self.wid = out[-1]
                break
        if not self.wid:
            raise RuntimeError("the app has no window on the display")
        subprocess.run(["xdotool", "windowfocus", self.wid], capture_output=True)
        time.sleep(0.3)
        geo = dict(l.split("=") for l in subprocess.run(["xdotool", "getwindowgeometry", "--shell", self.wid], capture_output=True, text=True).stdout.split())
        self.origin = (int(geo["X"]), int(geo["Y"]))

    def stop(self):
        if self.proc and self.proc.poll() is None:
            self.proc.kill()
            self.proc.wait(3)

    def alive(self):
        return self.proc.poll() is None

    def errors(self):
        with open(self.err_path, errors="replace") as f:
            return [l for l in f.read().splitlines() if any(w in l for w in ("panick", "error", "fail"))][:5]

    def layout(self):
        try:
            with open(self.layout_file) as f:
                return json.load(f)
        except (OSError, ValueError):
            return None

    @property
    def size(self):
        w, h = self.layout()["client"]
        return w, h

    # ---- input
    def mouse(self, x0, y0, x1, y1):
        ox, oy = self.origin
        cmd = ["xdotool", "mousemove", str(int(ox + x0)), str(int(oy + y0)), "sleep", "0.08", "mousedown", "1"]
        for i in range(1, 13):
            cmd += ["mousemove", str(int(ox + x0 + (x1 - x0) * i / 12)), str(int(oy + y0 + (y1 - y0) * i / 12)), "sleep", "0.01"]
        cmd += ["mouseup", "1"]
        subprocess.run(cmd, capture_output=True)
        time.sleep(0.15)

    def _emit(self, device, ident, phase, flags, px, pressure, contact=0):
        line = f"0 ptr {device} {ident} {phase} {flags} {contact} 1 {px[0]} {px[1]} {pressure} 0\n"
        with open(self.inject_file, "a") as f:
            f.write(line)

    def _px(self, p):
        w, h = self.size
        return round(p[0] * w), round(p[1] * h)

    def pen(self, path):
        pts = [self._px(p) for p in path]
        self._emit("pen", 1, "hover", 1, pts[0], 0)
        time.sleep(0.03)
        self._emit("pen", 1, "down", 3, pts[0], 400 / 1024)
        for i, p in enumerate(pts[1:]):
            time.sleep(PACE)
            self._emit("pen", 1, "move", 3, p, (300 + (i * 7) % 500) / 1024)
        time.sleep(PACE)
        self._emit("pen", 1, "up", 1, pts[-1], 0)
        time.sleep(0.02)
        self._emit("pen", 1, "leave", 0, pts[-1], 0)
        time.sleep(0.12)

    def finger(self, paths):
        pts = [[self._px(p) for p in path] for path in paths]
        steps = max(len(p) for p in pts)
        for f, path in enumerate(pts):
            self._emit("touch", f + 1, "down", 0, path[0], 0.5, 16)
        for k in range(1, steps):
            time.sleep(PACE)
            for f, path in enumerate(pts):
                self._emit("touch", f + 1, "move", 0, path[min(k, len(path) - 1)], 0.5, 16)
        time.sleep(PACE)
        for f, path in enumerate(pts):
            self._emit("touch", f + 1, "up", 0, path[-1], 0.5, 16)
        time.sleep(0.12)

    def keys(self, spec):
        keys = "+".join(KEYS.get(p, p) for p in spec.lower().split("+"))
        subprocess.run(["xdotool", "key", "--clearmodifiers", keys], capture_output=True)
        time.sleep(0.15)

    def type_text(self, text):
        subprocess.run(["xdotool", "type", "--delay", "40", "--", text], capture_output=True)
        time.sleep(0.15)

    # ---- looking
    def screenshot(self):
        w, h = self.size
        ox, oy = self.origin
        raw = subprocess.run(["import", "-window", "root", "-crop", f"{w}x{h}+{ox}+{oy}", "+repage", "-depth", "8", "rgba:-"], capture_output=True).stdout
        if len(raw) != w * h * 4:
            raise RuntimeError(f"a picture of {len(raw)} bytes for a window of {w}x{h}")
        return Image(w, h, raw, order=(0, 1, 2))
