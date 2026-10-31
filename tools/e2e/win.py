"""Windows: the real mouse and keyboard through SendInput, a synthetic stylus and finger through `inkpdf-bot` (Windows' own synthetic pointer
devices), screenshots of the window's client area. The release build under test is read from `inkpdf-build` beside the repository
(CARGO_TARGET_DIR if you build elsewhere): `cargo build --release -p inkpdf`. Scratch files go to `inkpdf-e2e` beside it (or INKPDF_E2E_DIR);
nothing is written inside the repository, and APPDATA points into the scratch folder, never the user's settings."""

import ctypes
import ctypes.wintypes as wt
import json
import os
import shutil
import subprocess
import time

from core import Driver, Image

user32 = ctypes.WinDLL("user32", use_last_error=True)
gdi32 = ctypes.WinDLL("gdi32", use_last_error=True)
user32.SetProcessDPIAware()

HANDLE = ctypes.c_void_p
user32.GetDC.restype = HANDLE
user32.GetDC.argtypes = [HANDLE]
user32.ReleaseDC.argtypes = [HANDLE, HANDLE]
user32.GetClientRect.argtypes = [HANDLE, ctypes.POINTER(wt.RECT)]
user32.ClientToScreen.argtypes = [HANDLE, ctypes.POINTER(wt.POINT)]
user32.SetForegroundWindow.argtypes = [HANDLE]
user32.IsWindowVisible.argtypes = [HANDLE]
user32.GetWindowTextW.argtypes = [HANDLE, wt.LPWSTR, ctypes.c_int]
user32.GetWindowThreadProcessId.argtypes = [HANDLE, ctypes.POINTER(wt.DWORD)]
user32.SendMessageTimeoutW.argtypes = [HANDLE, wt.UINT, wt.WPARAM, wt.LPCWSTR, wt.UINT, wt.UINT, ctypes.POINTER(ctypes.c_size_t)]
user32.SendMessageTimeoutW.restype = ctypes.c_ssize_t
gdi32.CreateCompatibleDC.restype = HANDLE
gdi32.CreateCompatibleDC.argtypes = [HANDLE]
gdi32.CreateCompatibleBitmap.restype = HANDLE
gdi32.CreateCompatibleBitmap.argtypes = [HANDLE, ctypes.c_int, ctypes.c_int]
gdi32.SelectObject.restype = HANDLE
gdi32.SelectObject.argtypes = [HANDLE, HANDLE]
gdi32.BitBlt.argtypes = [HANDLE, ctypes.c_int, ctypes.c_int, ctypes.c_int, ctypes.c_int, HANDLE, ctypes.c_int, ctypes.c_int, wt.DWORD]
gdi32.GetDIBits.argtypes = [HANDLE, HANDLE, wt.UINT, wt.UINT, ctypes.c_void_p, ctypes.c_void_p, wt.UINT]
gdi32.DeleteObject.argtypes = [HANDLE]
gdi32.DeleteDC.argtypes = [HANDLE]


class BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = [("biSize", wt.DWORD), ("biWidth", wt.LONG), ("biHeight", wt.LONG), ("biPlanes", wt.WORD), ("biBitCount", wt.WORD),
                ("biCompression", wt.DWORD), ("biSizeImage", wt.DWORD), ("biXPelsPerMeter", wt.LONG), ("biYPelsPerMeter", wt.LONG),
                ("biClrUsed", wt.DWORD), ("biClrImportant", wt.DWORD)]


class KEYBDINPUT(ctypes.Structure):
    _fields_ = [("wVk", wt.WORD), ("wScan", wt.WORD), ("dwFlags", wt.DWORD), ("time", wt.DWORD), ("dwExtraInfo", ctypes.c_size_t)]


class MOUSEINPUT(ctypes.Structure):
    _fields_ = [("dx", wt.LONG), ("dy", wt.LONG), ("mouseData", wt.DWORD), ("dwFlags", wt.DWORD), ("time", wt.DWORD), ("dwExtraInfo", ctypes.c_size_t)]


class INPUT(ctypes.Structure):
    class _U(ctypes.Union):
        _fields_ = [("ki", KEYBDINPUT), ("mi", MOUSEINPUT)]

    _anonymous_ = ("u",)
    _fields_ = [("type", wt.DWORD), ("u", _U)]


KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_UNICODE = 0x1, 0x2, 0x4
VK = {"ctrl": 0x11, "shift": 0x10, "alt": 0x12, "esc": 0x1B, "del": 0x2E, "enter": 0x0D, "backspace": 0x08, "pgup": 0x21, "pgdn": 0x22,
      "end": 0x23, "home": 0x24, "=": 0xBB, "-": 0xBD, "left": 0x25, "up": 0x26, "right": 0x27, "down": 0x28}
EXTENDED = {"del", "pgup", "pgdn", "end", "home", "left", "up", "right", "down"}


def _key(vk, up, extended=False):
    flags = (KEYEVENTF_KEYUP if up else 0) | (KEYEVENTF_EXTENDEDKEY if extended else 0)
    i = INPUT(type=1)
    i.ki = KEYBDINPUT(vk, 0, flags, 0, 0)
    return i


def _send(items):
    arr = (INPUT * len(items))(*items)
    user32.SendInput(len(items), arr, ctypes.sizeof(INPUT))


class Windows(Driver):
    name = "windows"
    default_mode = "desktop"

    def __init__(self):
        here = os.path.dirname(os.path.abspath(__file__))
        self.repo = os.path.abspath(os.path.join(here, "..", ".."))
        workspace = os.path.dirname(self.repo)
        self.sp = os.environ.get("INKPDF_E2E_DIR") or os.path.join(workspace, "inkpdf-e2e")
        os.makedirs(self.sp, exist_ok=True)
        target = os.path.join(os.environ.get("CARGO_TARGET_DIR") or os.path.join(workspace, "inkpdf-build"), "release")
        self.exe, self.bot, self.gen = (os.path.join(target, n) for n in ("inkpdf.exe", "inkpdf-bot.exe", "inkpdf-gen.exe"))
        self.layout_file = os.path.join(self.sp, "layout.json")
        self.proc = None
        self.hwnd = None
        self.args = []
        self.file = None
        self._size = (0, 0)
        self._origin = (0, 0)

    # ---- the app
    def launch(self, settings, pdf, mode):
        self.stop()
        subprocess.run(["taskkill", "/F", "/IM", "inkpdf.exe"], capture_output=True)
        appdata = os.path.join(self.sp, "appdata")
        os.environ["APPDATA"] = appdata
        shutil.rmtree(os.path.join(appdata, "InkPDF"), ignore_errors=True)
        os.makedirs(os.path.join(appdata, "InkPDF"), exist_ok=True)
        # (The light look is always added, so the colours counted are the light ones.)
        with open(os.path.join(appdata, "InkPDF", "settings.txt"), "w", newline="\n") as f:
            f.write(settings + "appearance light\n")
        self.file = None
        if pdf:
            self.file = os.path.join(self.sp, f"work-{pdf}.pdf")
            self.make_pdf(pdf, self.file)
        self.args = ["--mode", mode] if mode else []
        self._start()

    def make_pdf(self, kind, out):
        import re

        if m := re.fullmatch(r"pages(\d+)", kind):
            args = [m.group(1)]
        elif m := re.fullmatch(r"outline(\d+)", kind):
            args = [m.group(1), "--outline"]
        elif kind == "form":
            args = ["1", "--form"]
        else:
            raise ValueError(kind)
        subprocess.run([self.gen, out, *args], capture_output=True, check=True)

    def relaunch(self):
        self.stop()
        self._start()

    def _start(self):
        if os.path.exists(self.layout_file):
            os.remove(self.layout_file)
        cmd = [self.exe] + ([self.file] if self.file else []) + ["--topmost", "--layout", self.layout_file] + self.args
        self.out = open(os.path.join(self.sp, "app.out.txt"), "wb")
        self.err_path = os.path.join(self.sp, "app.err.txt")
        self.err = open(self.err_path, "wb")
        self.proc = subprocess.Popen(cmd, stdout=self.out, stderr=self.err)
        self.hwnd = None
        for _ in range(80):
            time.sleep(0.15)
            self.hwnd = self._find_window()
            if self.hwnd and os.path.exists(self.layout_file):
                break
        user32.SetForegroundWindow(self.hwnd)
        time.sleep(0.3)
        self.title_part = os.path.basename(self.file) if self.file else "Untitled"
        self._measure()

    def _find_window(self):
        found = []

        @ctypes.WINFUNCTYPE(wt.BOOL, HANDLE, wt.LPARAM)
        def each(hwnd, _):
            pid = wt.DWORD()
            user32.GetWindowThreadProcessId(hwnd, ctypes.byref(pid))
            if pid.value == self.proc.pid and user32.IsWindowVisible(hwnd):
                buf = ctypes.create_unicode_buffer(256)
                user32.GetWindowTextW(hwnd, buf, 256)
                if buf.value:
                    found.append(hwnd)
                    return False
            return True

        user32.EnumWindows(each, 0)
        return found[0] if found else None

    def _measure(self):
        rect, pt = wt.RECT(), wt.POINT(0, 0)
        user32.GetClientRect(self.hwnd, ctypes.byref(rect))
        user32.ClientToScreen(self.hwnd, ctypes.byref(pt))
        self._size = (rect.right - rect.left, rect.bottom - rect.top)
        self._origin = (pt.x, pt.y)

    def stop(self):
        if self.proc and self.proc.poll() is None:
            self.proc.kill()
            self.proc.wait(3)

    def alive(self):
        return self.proc.poll() is None

    def errors(self):
        self.err.flush()
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
        return self._size

    # ---- input
    def mouse(self, x0, y0, x1, y1):
        ox, oy = self._origin
        user32.SetCursorPos(int(ox + x0), int(oy + y0))
        time.sleep(0.08)
        user32.mouse_event(0x2, 0, 0, 0, 0)
        for i in range(1, 13):
            user32.SetCursorPos(int(ox + x0 + (x1 - x0) * i / 12), int(oy + y0 + (y1 - y0) * i / 12))
            time.sleep(0.01)
        user32.mouse_event(0x4, 0, 0, 0, 0)
        time.sleep(0.15)

    def _bot(self, *ops):
        subprocess.run([self.bot, self.title_part, *map(str, ops)], capture_output=True)
        time.sleep(0.12)

    def pen(self, path):
        # (The bot draws a straight run of its own points; a path is given to it as the same points one `line` at a time would make, so a
        # stroke is handed over whole: its first and last points and the points between, as a polyline of tiny lines is not one stroke.)
        if len(path) == 2 and path[0] == path[1]:
            self._bot("tap", *path[0])
            return
        ops = ["path"]
        for fx, fy in path:
            ops += [fx, fy]
        self._bot(*ops)

    def finger(self, paths):
        if len(paths) == 1 and len(paths[0]) == 1:
            (fx, fy), = paths[0]
            self._bot("finger", "down", 1, fx, fy, "finger", "up", 1, fx, fy)
        elif len(paths) == 1:
            ops = ["touchpath", 1]
            for fx, fy in paths[0]:
                ops += [fx, fy]
            self._bot(*ops)
        else:
            ops = ["touchpaths", len(paths), len(paths[0])]
            for k in range(len(paths[0])):
                for p in paths:
                    ops += [p[k][0], p[k][1]]
            self._bot(*ops)

    def keys(self, spec):
        parts = spec.lower().split("+")
        codes = []
        for p in parts:
            if p in VK:
                codes.append((VK[p], p in EXTENDED))
            elif len(p) == 1 and p.isalnum():
                codes.append((ord(p.upper()), False))
            else:
                raise ValueError(spec)
        _send([_key(vk, False, ext) for vk, ext in codes] + [_key(vk, True, ext) for vk, ext in reversed(codes)])
        time.sleep(0.15)

    def type_text(self, text):
        items = []
        for ch in text:
            for up in (False, True):
                i = INPUT(type=1)
                i.ki = KEYBDINPUT(0, ord(ch), KEYEVENTF_UNICODE | (KEYEVENTF_KEYUP if up else 0), 0, 0)
                items.append(i)
        _send(items)
        time.sleep(0.15)

    def posture(self, name):
        r = ctypes.c_size_t()
        user32.SendMessageTimeoutW(self.hwnd, 0x1A, 0, name, 2, 3000, ctypes.byref(r))
        time.sleep(0.5)
        return True

    # ---- looking
    def screenshot(self):
        self._measure()
        w, h = self._size
        ox, oy = self._origin
        screen = user32.GetDC(None)
        mem = gdi32.CreateCompatibleDC(screen)
        bmp = gdi32.CreateCompatibleBitmap(screen, w, h)
        old = gdi32.SelectObject(mem, bmp)
        gdi32.BitBlt(mem, 0, 0, w, h, screen, ox, oy, 0x00CC0020 | 0x40000000)
        info = BITMAPINFOHEADER(40, w, -h, 1, 32, 0, 0, 0, 0, 0, 0)
        buf = ctypes.create_string_buffer(w * h * 4)
        gdi32.GetDIBits(mem, bmp, 0, h, buf, ctypes.byref(info), 0)
        gdi32.SelectObject(mem, old)
        gdi32.DeleteObject(bmp)
        gdi32.DeleteDC(mem)
        user32.ReleaseDC(None, screen)
        return Image(w, h, buf.raw, order=(2, 1, 0))  # (BGRA)
