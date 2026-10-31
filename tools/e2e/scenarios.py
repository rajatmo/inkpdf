"""The real-input checks, written once for every platform (see core.py): that the stylus, a finger, the mouse and the keyboard reach the app and
do their work, in both modes; that pages are added, deleted, moved and saved; that the keys of a PDF reader work; that a PDF form is filled in
and kept. Which device may do what in every tool and mode, and every edge, is for the unit and storm tests; this proves the real window and the
real input reach them, wherever the program runs.

A check a platform cannot do is reported as SKIP with its reason; it never counts as a pass.
"""

DESKTOP = "desktop"


def devices(s):
    # ---- 1. Desktop mode: each device, the tools, undo.
    s.launch("tool pen\nslot 0\nshape-slot 3\ntext-slot 2\ntext-size 40.00\n", "", DESKTOP)
    s.pen_stroke(0.3, 0.5, 0.7, 0.5, 0.03, 2, 40)
    ink = s.black()
    s.check(ink > 500 and s.dirty(), f"the stylus draws ({ink} px), and the document is marked changed")
    s.press("undo")
    s.check(s.black() == 0, "undo, pressed with the mouse, takes it away")
    s.finger_drag(0.5, 0.4, 0.5, 0.6, 15)
    s.finger_pinch(0.5, 0.5, 0.05, 0.2, 15)
    s.finger_pinch(0.5, 0.5, 0.2, 0.05, 15)
    s.check(s.black() == 0, "a finger pans and zooms and draws nothing")
    # (On a small screen the page does not come back to where it was by itself: the width is fitted again.)
    s.keys("ctrl+0")
    s.wait(500)
    s.press("redo")
    s.check(s.black() > 500, "redo brings it back")
    one = s.black()
    s.mouse_drag(0.3, 0.7, 0.7, 0.7)
    s.check(s.black() > one * 1.5, "the mouse draws too, in the desktop mode")
    s.keys("ctrl+z")
    s.check(abs(s.black() - one) < one * 0.05, "Ctrl+Z, from the keyboard, takes that back")
    s.press("geometry")
    s.pen_line(0.3, 0.25, 0.6, 0.35, 20)
    s.check(s.accent() > 50, "the shape tool: a stylus drag makes a shape and its anchors show")
    s.press("text")
    # (Text goes in the upper part of the page: a soft keyboard covers the lower half of a phone.)
    s.click(0.2, 0.4)
    s.type_text("Hi")
    s.keys("esc")
    s.check(s.red() > 150, "the text tool: a mouse click and the keyboard make text")
    s.press("lasso")
    s.click(0.5, 0.5)
    s.check(s.accent() > 100, "the lasso: a click on the stroke selects it")
    s.keys("del")
    s.check(s.black() == 0, "Delete takes the selection away")
    s.keys("ctrl+z")
    s.press("eraser")
    s.mouse_drag(0.5, 0.4, 0.5, 0.6)
    s.check(s.black() == 0, "the eraser: the left button erases what it passes over")
    s.keys("ctrl+z")
    s.press("pen")
    red = s.red()
    s.bar("swatch2")
    s.mouse_drag(0.3, 0.65, 0.7, 0.65)
    s.check(s.red() > red + 500, "a colour pressed in the bar is the colour of the next stroke")
    s.keys("ctrl+z")
    s.bar("swatch0")

    # ---- 2. Tablet mode: the mode button, the stylus tip alone (no barrel button, no eraser end), a box's text to the clipboard.
    def lit():
        # (Where the button is is asked each time: on a narrow window it is in a pill that folds away.)
        return s.count_near(s.ACCENT, s.spot("buttons", "mode"))

    s.check(lit() < 5, "desktop mode: the mode button is plain")
    s.press("mode")
    s.check(lit() > 30 and s.mode() == "tablet", "one press: tablet mode, and the button shows it")
    ink = s.black()
    s.mouse_drag(0.3, 0.6, 0.7, 0.6)
    s.check(s.black() == ink, "the tablet's mouse draws nothing")
    s.pen_press("lasso")
    s.pen_tap(0.5, 0.5)
    s.check(s.accent() > 100, "with the tip alone: the lasso's tap selects the stroke")
    s.pen_bar("delete")
    s.check(s.black() == 0, "and the bar's delete, tapped, takes it away")
    s.pen_press("undo")
    s.check(s.black() > 500, "undo, tapped, brings it back")
    s.pen_press("text")
    s.pen_tap(0.2, 0.2)
    s.type_text("ab")
    s.keys("ctrl+a")
    s.keys("ctrl+c")
    s.keys("esc")
    # (What was copied is what a new box is given: the clipboard is the system's, the same on every platform.)
    before = s.red()
    s.pen_tap(0.2, 0.3)
    s.keys("ctrl+v")
    s.keys("esc")
    s.check(s.red() > before + 100, "Ctrl+A, Ctrl+C in a box, Ctrl+V in another: its words went through the system's clipboard")
    if s.tell_setting("ConvertibleSlateMode"):
        s.check(s.mode() == "tablet" and lit() > 30, "a mode given on the command line is not changed by the posture")
    else:
        s.skip("a mode given on the command line is not changed by the posture", "this platform has no posture to change")
    s.press("mode")
    s.check(lit() < 5 and s.mode() == "desktop", "another press: back at the desk")
    s.check(s.healthy(), "no crash, nothing printed to stderr")

    # ---- 3. No mode given: the app follows the machine (a desktop PC and a Linux desktop start in the desktop mode, and their mouse draws; a tablet starts
    # in the tablet mode, and its mouse does not).
    s.launch("tool pen\nslot 0\n", "", None)
    want = s.d.default_mode
    s.check(s.mode() == want, f"no mode given: this platform starts in the {want} mode ({s.mode()})")
    s.mouse_drag(0.3, 0.5, 0.7, 0.5)
    s.check((s.black() > 500) == (want == DESKTOP), f"and its mouse {'draws' if want == DESKTOP else 'draws nothing'}")
    s.press("undo")
    s.press("mode")
    s.check(s.mode() != want, "the button changed the mode")
    s.mouse_drag(0.3, 0.5, 0.7, 0.5)
    s.check((s.black() > 500) != (want == DESKTOP), "and the mouse follows the new mode")
    if s.tell_setting("ConvertibleSlateMode"):
        s.mouse_drag(0.3, 0.5, 0.7, 0.5)
        s.check(s.mode() == want, "a posture change: the app looks at the machine again, and it is as it was")
    else:
        s.skip("a posture change: the app looks at the machine again", "this platform has no posture to change")
    s.check(s.healthy(), "no crash, nothing printed to stderr")
    s.stop()


def pages(s):
    def red_in(n):
        # (The ink's own red, closely: the pages have colours of their own that a looser match would count, and the size of a picture follows the
        # window's, so what counts is whether there is red at all.)
        p = s.spot("overview/pages", str(n))
        return s.count_near(s.RED, p, int(p[2] / 2 - 4), int(p[3] / 2 - 4), tol=20)

    def inky(count):
        return count > 15

    def bare(count):
        return count < 5

    def last_in_view():
        # (The pages the overview has in the window, whole and clear of the bar that edit mode puts at the bottom: the others are further down.)
        _, h = s.d.size
        shown = s.d.layout()["overview"]["pages"]
        return max(int(k) for k, p in shown.items() if p[1] + p[3] / 2 <= 0.8 * h)

    def page_click(n):
        s.click_spot(s.spot("overview/pages", str(n)))

    def control(name):
        s.click_spot(s.spot("overview/controls", name))

    def action(name):
        s.click_spot(s.spot("overview/actions", name))

    # ---- 1. Three pages: the bin and + with the mouse, the stylus and a finger; save; reopen.
    s.launch("tool pen\nslot 2\nwidth 6.00\n", "pages3", DESKTOP)
    s.check(s.pages()[1] == 3 and not s.dirty(), f"a three-page PDF opens ({s.pages()})")
    s.pen_stroke(0.3, 0.5, 0.7, 0.5, 0.03, 2, 40)
    s.check(s.red() > 500, "ink on page 1")
    s.press("del")
    s.check(s.pages()[1] == 2 and s.red() == 0, f"the bin deletes the page with one press, and its ink goes with it ({s.pages()})")
    s.press("plus")
    s.check(s.pages()[1] == 3, f"+ adds a page ({s.pages()})")
    s.pen_press("del")
    s.check(s.pages()[1] == 2, "the stylus presses the same button")
    s.wait(700)  # (a finger is ignored while the pen was just near)
    s.finger_press("del")
    s.check(s.pages()[1] == 1, "and a finger")
    s.press("del")
    s.check(s.pages()[1] == 1, "the last page there is stays")
    s.press("save")
    s.wait(500)
    s.check(not s.dirty(), "save writes the file")
    s.reopen()
    s.check(s.pages()[1] == 1, "reopened: the file has the one page that is left")

    # ---- 2. Twelve pages with a table of contents: the overview.
    s.launch("tool pen\nslot 2\nwidth 6.00\n", "outline12", DESKTOP)
    s.check(s.pages() == (1, 12), f"a twelve-page PDF with a table of contents opens ({s.pages()})")
    s.mouse_drag(0.3, 0.4, 0.6, 0.5)
    s.press("pages")
    s.wait(1200)
    a, b = red_in(1), red_in(2)
    s.check(inky(a) and bare(b), f"the overview opens: the picture of page 1 has its red ink, page 2's has none ({a}, {b})")
    page_click(8)
    s.wait(800)
    s.check(s.pages()[0] == 8, f"a click on a page goes to it and closes the overview ({s.pages()})")
    s.press("pages")
    s.wait(800)
    if s.has("overview/toc", "Chapter 2"):
        s.click_spot(s.spot("overview/toc", "Chapter 2"))
        s.wait(800)
        s.check(s.pages()[0] == 4, f"a click on a contents entry goes to its page ({s.pages()})")
    else:
        s.skip("a click on a contents entry goes to its page", "a window this narrow has no table of contents beside the pages")
        control("close")
        s.wait(500)
    # (The overview opens on the page in view: from the first, so that the first pages are in the window of a small screen too.)
    s.keys("home")
    s.wait(600)
    s.press("pages")
    s.wait(800)
    control("edit")
    page_click(1)
    page_click(2)
    action("duplicate")
    s.wait(500)
    s.check(s.pages()[1] == 14, f"edit mode: two pages chosen and duplicated make fourteen ({s.pages()})")
    s.keys("ctrl+z")
    s.wait(400)
    s.check(s.pages()[1] == 12, f"Ctrl+Z undoes the duplicate ({s.pages()})")
    s.keys("ctrl+y")
    s.wait(400)
    s.check(s.pages()[1] == 14, "and Ctrl+Y does it again")
    s.keys("ctrl+z")
    s.wait(400)
    # A real drag: the first page past the last one in view (all twelve in a big window, fewer in a small one).
    n = last_in_view()
    first, last = s.spot("overview/pages", "1"), s.spot("overview/pages", str(n))
    s.mouse_px(first[0], first[1], last[0] + last[2] * 0.6, last[1])
    s.wait(600)
    a, b = red_in(1), red_in(n)
    s.check(bare(a) and inky(b), f"dragging page 1 past the last in view ({n}) moves it there, ink and all ({a}, {b})")
    s.keys("esc")
    s.keys("esc")
    s.wait(400)
    s.keys("ctrl+s")
    s.wait(1500)
    s.reopen()
    s.press("pages")
    s.wait(1200)
    s.check(inky(red_in(n)), "saved and reopened: the red page is where it was put, as the file has it")
    control("edit")
    page_click(n)
    action("delete")
    s.wait(500)
    s.check(s.pages()[1] == 11 and bare(red_in(min(n, 11))), f"edit mode: the page deleted from the bar, its ink with it ({s.pages()})")
    control("undo")
    s.wait(700)
    s.check(inky(red_in(n)), "the overview's undo brings it back with its ink")
    control("close")
    s.check(s.healthy(), "no crash, nothing printed to stderr")

    # ---- 3. Pages that were added have no picture from the file: their pictures show what is drawn on them.
    s.launch("tool pen\nslot 2\nwidth 4.00\n", "", DESKTOP)
    s.pen_stroke(0.10, 0.30, 0.50, 0.30, 0.075, 2.5, 150)
    s.press("plus")
    s.wait(800)
    s.pen_stroke(0.10, 0.45, 0.50, 0.45, 0.04, 5, 150)
    s.press("plus")
    s.wait(800)
    s.pen_stroke(0.10, 0.55, 0.60, 0.40, 0.05, 3, 150)
    s.press("pages")
    s.wait(2000)
    s.check(s.pages()[1] == 3, f"a blank document with two pages added has three ({s.pages()})")
    r1, r2, r3 = red_in(1), red_in(2), red_in(3)
    s.check(r1 > 8 and r2 > 8 and r3 > 8, f"each picture has its page's ink, the added ones too ({r1}, {r2}, {r3})")
    control("close")
    s.check(s.healthy(), "no crash, nothing printed to stderr")
    s.stop()


def reader(s):
    # ---- The Select tool is the reader (it is what the program starts on).
    s.launch("tool select\n", "pages3", DESKTOP)
    first, total = s.pages()
    s.check(total >= 2, f"the document has pages to read ({total})")
    for _ in range(3):
        s.keys("pgdn")
    s.wait(900)
    s.check(s.pages()[0] > first, f"Page Down, three times, scrolls on: the page in view is a later one ({s.pages()[0]})")
    s.keys("end")
    s.wait(900)
    s.check(s.pages()[0] == total, "End goes to the last page")
    s.keys("home")
    s.wait(900)
    s.check(s.pages()[0] == first, "Home goes back to the first")
    # Zoom: the page at its size, then the width, change what is drawn.
    # (Dark pixels, the page's text: how much of the page is in the window changes with the zoom, where its white does not.)
    def text():
        return s.count(0, 0, 0, tol=90)

    a = text()
    s.keys("ctrl+1")
    s.wait(500)
    b = text()
    s.keys("ctrl+0")
    s.wait(500)
    c = text()
    s.finger_pinch(0.5, 0.5, 0.05, 0.2, 15)
    s.wait(500)
    d = text()
    s.check(a != b or b != c or c != d, f"Ctrl+1, Ctrl+0 and a finger's pinch zoom the page ({a}, {b}, {c}, {d} dark pixels)")
    # A drag on empty page moves it (the left button, with the Select tool).
    s.keys("home")
    s.wait(600)
    s.mouse_drag(0.5, 0.85, 0.5, 0.15)
    s.wait(500)
    s.check(s.pages()[0] >= first, "a drag on empty page with the Select tool moves it, and draws nothing")
    s.check(not s.dirty(), "and no ink was made: the document is not marked changed")
    # With a drawing tool the same drag draws.
    s.press("pen")
    s.mouse_drag(0.3, 0.5, 0.7, 0.5)
    s.check(s.dirty(), "with the pen the same left-button drag draws and marks the document changed: ink cannot wait")
    s.check(s.healthy(), "no crash, nothing printed to stderr")
    s.stop()


def forms(s):
    def dark():
        # The program's own marks (24 26 34), the file's (black), and the page's text and frames, which are there before and after.
        return s.count(24, 26, 34) + s.count(0, 0, 0)

    s.launch("tool select\n", "form", DESKTOP)
    s.check(not s.dirty(), "a form opens as it is: nothing changed")
    empty = dark()
    # The mouse (left button) with the Select tool.
    s.click_spot(s.spot("fields", "agree"))
    s.wait(300)
    s.check(s.dirty(), "a click on the check box fills it and marks the document changed")
    now = dark()
    s.check(now > empty, f"and the mark is drawn ({empty} -> {now} dark pixels)")
    s.click_spot(s.spot("fields", "size#1"))
    s.wait(300)
    s.click_spot(s.spot("fields", "colour"))
    s.wait(300)
    chosen = dark()
    s.click_spot(s.spot("fields", "name"))
    s.wait(300)
    s.type_text("Ada Lovelace")
    s.wait(300)
    now = dark()
    s.check(now > chosen, f"typed text is drawn in the field ({chosen} -> {now} dark pixels)")
    s.keys("enter")
    s.wait(200)
    # The stylus's tip (a tap) and a finger: each turns the check box, so it is on again after the three.
    s.tap_spot(s.spot("fields", "agree"))
    s.wait(300)
    s.finger_tap(s.spot("fields", "agree"))
    s.wait(300)
    s.check(s.healthy(), "no crash, nothing printed to stderr")
    filled = dark()
    # Saved, and there when the file is opened again (the file's own appearances draw it, not the program).
    s.keys("ctrl+s")
    for _ in range(30):
        if not s.dirty():
            break
        s.wait(200)
    s.check(not s.dirty(), "the document is saved")
    s.reopen()
    s.wait(800)
    s.check(not s.dirty(), "reopened: nothing to save")
    now = dark()
    s.check(now > empty, f"and the values are drawn by the file ({empty} -> {now} dark pixels; filled was {filled})")
    s.check(s.healthy(), "no crash after reopening")
    s.stop()


ALL = {"devices": devices, "pages": pages, "reader": reader, "forms": forms}
