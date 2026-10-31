"""Android, on an emulator or a device on adb (the APK is installed by run.ps1): input through the system's own input stack. The stylus and the
fingers (one or several, so a pinch) are MotionEvents sent through the input dispatcher by a small driver app (tools/android/driver, an
instrumentation: `adb shell input` can send one pointer only), the mouse and the keyboard go through `adb shell input`, screenshots are
`screencap`. The app is told what a test run needs through files in its own folder (layout.on: write where the buttons are; mode.txt: the mode
to keep; InkPDF/settings.txt: the settings to start with), as a desktop run is told with command-line options, and is handed a PDF with a VIEW
intent as another app would.

The mouse only draws in the desktop mode, and a phone or tablet starts in the tablet mode: scenarios that need the mouse say `desktop`."""

import json
import os
import re
import subprocess
import tempfile
import time

from core import Driver, Image

PACKAGE = "app.inkpdf.test"
DATA = f"/sdcard/Android/data/{PACKAGE}/files"
DRIVER = "app.inkpdf.driver"
KEYCODES = {"ctrl": "KEYCODE_CTRL_LEFT", "shift": "KEYCODE_SHIFT_LEFT", "alt": "KEYCODE_ALT_LEFT", "esc": "KEYCODE_ESCAPE", "del": "KEYCODE_FORWARD_DEL",
            "enter": "KEYCODE_ENTER", "backspace": "KEYCODE_DEL", "pgup": "KEYCODE_PAGE_UP", "pgdn": "KEYCODE_PAGE_DOWN", "end": "KEYCODE_MOVE_END",
            "home": "KEYCODE_MOVE_HOME", "=": "KEYCODE_EQUALS", "-": "KEYCODE_MINUS", "left": "KEYCODE_DPAD_LEFT", "up": "KEYCODE_DPAD_UP", "right": "KEYCODE_DPAD_RIGHT", "down": "KEYCODE_DPAD_DOWN"}


class Android(Driver):
    name = "android"
    default_mode = "tablet"

    def __init__(self):
        sdk = os.environ.get("ANDROID_HOME") or os.path.join(os.environ.get("LOCALAPPDATA", ""), "Android", "Sdk")
        self.adb = os.path.join(sdk, "platform-tools", "adb.exe" if os.name == "nt" else "adb")
        here = os.path.dirname(os.path.abspath(__file__))
        workspace = os.path.dirname(os.path.abspath(os.path.join(here, "..", "..")))
        self.sp = os.environ.get("INKPDF_E2E_DIR") or os.path.join(workspace, "inkpdf-e2e")
        os.makedirs(self.sp, exist_ok=True)
        target = os.path.join(os.environ.get("CARGO_TARGET_DIR") or os.path.join(workspace, "inkpdf-build"), "release")
        self.gen = os.path.join(target, "inkpdf-gen.exe" if os.name == "nt" else "inkpdf-gen")
        self.driver_apk = os.path.join(workspace, "inkpdf-dist", "InkPDF-input-driver.apk")
        self.file = None
        self._layout = (0.0, None)
        self._driver_ready = False
    # ---- adb
    def sh(self, command, binary=False):
        out = subprocess.run([self.adb, "shell", command], capture_output=True).stdout
        return out if binary else out.decode(errors="replace").replace("\r", "")

    def exec_out(self, *argv):
        return subprocess.run([self.adb, "exec-out", *argv], capture_output=True).stdout

    def push_text(self, text, remote):
        with tempfile.NamedTemporaryFile("w", delete=False, suffix=".txt", newline="\n") as f:
            f.write(text)
        try:
            subprocess.run([self.adb, "push", f.name, remote], capture_output=True)
        finally:
            os.remove(f.name)

    # ---- the app
    def launch(self, settings, pdf, mode):
        self.stop()
        m = re.search(r"(\d+)x(\d+)", self.sh("wm size").strip().splitlines()[-1])
        self.screen = (int(m.group(1)), int(m.group(2)))
        self._ensure_driver()
        self.sh(f"mkdir -p {DATA}/InkPDF; rm -f {DATA}/layout.json {DATA}/layout.part {DATA}/mode.txt {DATA}/*.pdf")
        self.push_text("", f"{DATA}/layout.on")
        self.push_text(settings + "appearance light\n", f"{DATA}/InkPDF/settings.txt")
        if mode:
            self.push_text(mode + "\n", f"{DATA}/mode.txt")
        self.file = None
        if pdf:
            local = os.path.join(self.sp, f"work-{pdf}.pdf")
            self.make_pdf(pdf, local)
            self.file = f"{DATA}/work-{pdf}.pdf"
            subprocess.run([self.adb, "push", local, self.file], capture_output=True)
        self._start()

    def _ensure_driver(self):
        """The input driver app is on the device (installed from inkpdf-dist, built by tools/android/build-driver.ps1)."""
        if self._driver_ready:
            return
        if DRIVER not in self.sh(f"pm list packages {DRIVER}"):
            subprocess.run([self.adb, "install", "-r", self.driver_apk], capture_output=True, check=True)
        self._driver_ready = True
    def make_pdf(self, kind, out):
        if m := re.fullmatch(r"pages(\d+)", kind):
            args = [m.group(1)]
        elif m := re.fullmatch(r"outline(\d+)", kind):
            args = [m.group(1), "--outline"]
        elif m := re.fullmatch(r"links(\d+)", kind):
            args = [m.group(1), "--links"]
        elif kind == "form":
            args = ["1", "--form"]
        elif kind == "broken":
            # (A file that is no PDF: the program has to say so, not leave a window with nothing in it.)
            with open(out, "wb") as f:
                f.write(b"this is not a PDF\n")
            return
        else:
            raise ValueError(kind)
        subprocess.run([self.gen, out, *args], capture_output=True, check=True)

    def relaunch(self):
        self.stop()
        self.sh(f"rm -f {DATA}/layout.json")
        self._start()

    def _start(self):
        self.sh("logcat -c")
        self._layout = (0.0, None)
        view = f" -a android.intent.action.VIEW -d file://{self.file} -t application/pdf" if self.file else ""
        self.sh(f"am start -n {PACKAGE}/.Host{view}")
        for _ in range(200):
            time.sleep(0.2)
            lay = self.layout(fresh=True)
            if lay and "state" in lay:
                break
        time.sleep(0.5)

    def stop(self):
        self.sh(f"am force-stop {PACKAGE}")

    def alive(self):
        return bool(self.sh(f"pidof {PACKAGE}").strip())

    def errors(self):
        log = self.sh("logcat -d -s inkpdf")
        return [l for l in log.splitlines() if any(w in l for w in ("panicked", "FATAL"))][:5]

    def layout(self, fresh=False):
        # (A read is a call to adb: the same answer is kept for a moment, unless the caller says it wants the latest.)
        at, kept = self._layout
        if not fresh and kept is not None and time.time() - at < 0.15:
            return kept
        try:
            kept = json.loads(self.exec_out("cat", f"{DATA}/layout.json"))
        except ValueError:
            kept = None
        self._layout = (time.time(), kept)
        return kept

    @property
    def size(self):
        w, h = self.layout()["client"]
        return w, h

    # ---- input
    def _px(self, p):
        w, h = self.size
        return round(p[0] * w), round(p[1] * h)

    def _drive(self, ops):
        """Hand the driver app a script of pointer operations (see Drive.java) and wait for it to be played."""
        out = self.sh(f"am instrument -w -e ops '{';'.join(ops)}' {DRIVER}/.Drive")
        if "result=ok" not in out:
            raise RuntimeError(f"the input driver failed: {out.strip()}")
        time.sleep(0.15)
    def _contacts(self, tool, paths, pressure, pace_ms):
        """Fingers or the pen down together at the start of each path, moving along them, and up."""
        pts = [[self._px(p) for p in path] for path in paths]
        steps = max(len(p) for p in pts)
        ops = [f"D {i} {tool} {path[0][0]} {path[0][1]} {pressure(0)}" for i, path in enumerate(pts)]
        for k in range(1, steps):
            ops.append(f"W {pace_ms}")
            ops += [f"M {i} {path[min(k, len(path) - 1)][0]} {path[min(k, len(path) - 1)][1]} {pressure(k)}" for i, path in enumerate(pts)]
            ops.append("F")
        ops.append(f"W {pace_ms}")
        ops += [f"U {i}" for i in reversed(range(len(pts)))]
        self._drive(ops)

    def pen(self, path):
        self._contacts("pen", [path], lambda k: 0.4 if k == 0 else (300 + (k * 7) % 500) / 1024, 5)

    def finger(self, paths):
        self._contacts("finger", paths, lambda k: 0.5, 8)
    def mouse(self, x0, y0, x1, y1):
        if (x0, y0) == (x1, y1):
            self.sh(f"input mouse tap {int(x0)} {int(y0)}")
        else:
            self.sh(f"input mouse swipe {int(x0)} {int(y0)} {int(x1)} {int(y1)} 250")
        time.sleep(0.2)

    def activate(self, p):
        # (A finger works in both modes; the mouse only in the desktop mode.)
        w, h = self.size
        f = (p[0] / w, p[1] / h)
        self.finger([[f, f]])

    def keys(self, spec):
        codes = []
        for p in spec.lower().split("+"):
            if p in KEYCODES:
                codes.append(KEYCODES[p])
            elif len(p) == 1 and p.isalnum():
                codes.append(f"KEYCODE_{p.upper()}")
            else:
                raise ValueError(spec)
        self.sh(f"input keyevent {codes[0]}" if len(codes) == 1 else f"input keycombination {' '.join(codes)}")
        time.sleep(0.2)

    def type_text(self, text):
        self.sh("input text '" + text.replace(" ", "%s") + "'")
        time.sleep(0.2)

    # ---- looking
    def screenshot(self):
        raw = self.exec_out("screencap")
        w, h = int.from_bytes(raw[0:4], "little"), int.from_bytes(raw[4:8], "little")
        header = len(raw) - w * h * 4  # (12 bytes on older systems, 16 on newer)
        return Image(w, h, raw[header:], order=(0, 1, 2))
