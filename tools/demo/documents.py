"""The documents the tour opens: a short paper with a table of contents, a slide deck, and a registration form. They are made fresh each run
(PyMuPDF and the fonts Windows ships), so the tour shows real-looking pages and nothing binary lives in the repository.

    python tools/demo/documents.py <folder>
"""

import os
import sys

import pymupdf

FONTS = r"C:\Windows\Fonts"
SERIF, SERIF_B, SERIF_I = "georgia.ttf", "georgiab.ttf", "georgiai.ttf"
SANS, SANS_SB, SANS_B = "segoeui.ttf", "seguisb.ttf", "segoeuib.ttf"

INK = (0.13, 0.15, 0.19)
MUTED = (0.42, 0.45, 0.52)
ACCENT = (0.16, 0.38, 0.80)
RULE = (0.85, 0.87, 0.91)


def font(page, name):
    page.insert_font(fontname=name.split(".")[0], fontfile=os.path.join(FONTS, name))
    return name.split(".")[0]


def text(page, rect, s, name, size, color=INK, align=0, lh=1.35):
    fn = font(page, name)
    r = pymupdf.Rect(rect)
    r.y1 = max(r.y1, r.y0 + size * lh * 1.4)
    for _ in range(20):
        left = page.insert_textbox(r, s, fontname=fn, fontsize=size, color=color, align=align, lineheight=lh)
        if left >= 0:
            return left
        r.y1 += size * lh  # (nothing is written while it does not fit)
    raise ValueError(f"text does not fit: {s[:40]}")


# ---- The paper
PAPER = [
    ("Introduction", [
        "Paper is still the fastest way to think with your hands. You circle what matters, scribble a question in the margin and draw an arrow "
        "to the answer, all without looking away from the page. A good annotator should feel the same: the ink appears under the tip, the "
        "page never gets in the way, and what you write stays in the file.",
        "This note describes the choices behind a pen-first PDF reader: how strokes are captured, how the toolbar stays out of the way, and "
        "how every action works with a single kind of input, whether that is a stylus, a finger, or a mouse and keyboard.",
    ]),
    ("Latency", [
        "The time from a pen sample to a lit pixel decides whether ink feels attached to the pen or dragged behind it. Below roughly ten "
        "milliseconds most people stop noticing the gap; above thirty it becomes the first thing they see.",
        "The pen is therefore read as late as possible before each frame, and only the newest part of a stroke is drawn again. Pages are "
        "cached on the graphics card, so an idle window costs nothing at all.",
    ]),
    ("Pressure and Shape", [
        "A stroke is more than a line: its width follows the pressure of the tip, and it is smoothed without losing the corners a hand puts "
        "into handwriting. Shapes are different. A rectangle or an arrow should be exact, and every node should stay editable afterwards.",
        "Both are saved as ordinary PDF annotations, so any reader shows them, and nothing is flattened into the page.",
        "Colour and thickness are chosen from a short row of swatches under the tools. A selection can be recoloured, duplicated, copied "
        "or deleted from the same row, so the common edits never need a menu.",
    ]),
    ("One Input Is Enough", [
        "A tablet on a train has a stylus and fingers; a laptop on a desk has a mouse and keys. Every tool works with either set alone: "
        "the stylus tip or the left button draws, selects and erases, while fingers pan, zoom and press the buttons.",
        "When the screen is narrow the toolbar folds into round buttons, and each one opens its pill only when it is needed.",
    ]),
    ("Reading", [
        "Annotation starts with reading. Text can be selected, copied, highlighted, underlined and struck out; search lights every match "
        "and lists them; links jump and come back; forms are filled in place.",
        "The pages overview shows every page at once, with the table of contents beside it, so a long document is one glance away.",
        "Pages can be reordered, duplicated, deleted or flagged there, and every step can be undone. A flag is an ordinary bookmark, "
        "so other readers see it too.",
    ]),
    ("Conclusion", [
        "A PDF annotator earns its place by disappearing. When the ink keeps up with the pen and the tools fold away, what remains is the "
        "page and the person writing on it.",
        "The same reader also presents: one button shows the document as slides, a whole page at a time over black, turned by a click, a "
        "tap, a swipe or the arrow keys.",
    ]),
    ("References", [
        "[1] Notes on stroke smoothing and pressure curves, internal report.   [2] Measuring pen-to-pixel latency with a high-speed "
        "camera, lab notes.   [3] Form fields in the PDF 2.0 specification, sections 12.7 and 12.8.",
    ]),
]


def latency_figure(page, r):
    """A small bar chart: where the milliseconds go between the pen and the screen."""
    r = pymupdf.Rect(r)
    page.draw_rect(r, color=RULE, fill=(0.975, 0.98, 0.99), width=0.6, radius=0.03)
    bars = [("Pen sample", 1.0), ("Smoothing", 0.4), ("Draw", 0.7), ("Present", 0.4)]
    x0, y0 = r.x0 + 92, r.y0 + 22
    scale = (r.width - 140) / 2.5
    for i, (label, ms) in enumerate(bars):
        y = y0 + i * 22
        text(page, (r.x0 + 12, y - 1, x0 - 6, y + 16), label, SANS, 8.5, MUTED, align=2)
        page.draw_rect((x0, y + 2, x0 + ms * scale, y + 13), color=None, fill=ACCENT if i != 2 else (0.25, 0.62, 0.45), radius=0.25)
        text(page, (x0 + ms * scale + 5, y - 1, x0 + ms * scale + 60, y + 16), f"{ms:.1f} ms", SANS, 8, MUTED)
    text(page, (r.x0 + 12, r.y1 - 20, r.x1 - 12, r.y1 - 4), "Figure 1. Pen sample to frame, about 2.5 ms in all.", SERIF_I, 8, MUTED)


def paper(out):
    doc = pymupdf.open()
    W, H = pymupdf.paper_size("letter")
    M = 72
    toc = []
    sections = list(enumerate(PAPER, 1))
    # Two sections a page after the first, one on the first page with the title and the figure.
    layout = [[sections[0]], sections[1:3], sections[3:5], sections[5:7]]
    for pno, group in enumerate(layout, 1):
        page = doc.new_page(width=W, height=H)
        y = M
        # Running head and folio.
        text(page, (M, 36, W - M, 52), "Writing on PDFs with a Pen", SANS, 8, MUTED)
        text(page, (M, 36, W - M, 52), str(pno), SANS, 8, MUTED, align=2)
        page.draw_line((M, 54), (W - M, 54), color=RULE, width=0.6)
        if pno == 1:
            text(page, (M, y, W - M, y + 70), "Writing on PDFs with a Pen", SERIF_B, 26, INK, lh=1.2)
            y += 40
            text(page, (M, y, W - M, y + 20), "Design notes for a pen-first annotator", SERIF_I, 13, MUTED)
            y += 30
            page.draw_rect((M, y, W - M, y + 74), color=None, fill=(0.95, 0.965, 0.99), radius=0.06)
            page.draw_line((M + 1, y + 6), (M + 1, y + 68), color=ACCENT, width=2.5)
            text(page, (M + 16, y + 10, W - M - 16, y + 72),
                 "Abstract. Ink should appear under the pen, tools should fold away, and every action should work with whatever input is at "
                 "hand. This note explains how those three ideas shape the reader, from latency to forms.", SERIF, 10, INK, lh=1.45)
            y += 96
        for n, (title, paras) in group:
            toc.append([1, f"{n}  {title}", pno, y])
            text(page, (M, y, W - M, y + 24), f"{n}   {title}", SANS_SB, 15, INK)
            y += 28
            for p in paras:
                lines = len(p) // 92 + 1
                hgt = lines * 15.5 + 14
                text(page, (M, y, W - M, y + hgt), p, SERIF, 10.5, INK, align=3, lh=1.5)
                y += hgt - 4
            y += 14
            if pno == 1:
                latency_figure(page, (M + 40, y, W - M - 40, y + 130))
                y += 150
            if n == 2:
                quote = "\u201cBelow ten milliseconds, the ink feels attached to the pen.\u201d"
                page.draw_line((M + 30, y), (M + 30, y + 40), color=ACCENT, width=2)
                text(page, (M + 44, y + 2, W - M - 30, y + 44), quote, SERIF_I, 14, ACCENT, lh=1.3)
                y += 62
            if n == 4:
                rows = [("Device", "Draws", "Pans and zooms", "Presses buttons"), ("Stylus tip", "yes", "no", "yes"),
                        ("Finger", "with the finger button", "yes", "yes"), ("Mouse and keys", "yes", "wheel, keys", "yes")]
                cols = [M, M + 130, M + 280, M + 380]
                for i, row in enumerate(rows):
                    yy = y + i * 22
                    if i == 0:
                        page.draw_rect((M, yy - 3, W - M, yy + 17), color=None, fill=(0.95, 0.965, 0.99))
                    for c, cell in zip(cols, row):
                        text(page, (c + 6, yy, c + 150, yy + 18), cell, SANS_SB if i == 0 else SANS, 9.5, INK)
                    page.draw_line((M, yy + 18), (W - M, yy + 18), color=RULE, width=0.5)
                y += 4 * 22 + 20
    doc.set_toc([[lvl, t, p, {"kind": pymupdf.LINK_GOTO, "page": p - 1, "to": pymupdf.Point(0, yy), "zoom": 0}] for lvl, t, p, yy in toc])
    doc.save(out, garbage=3, deflate=True)


# ---- The slides
NAVY, SKY, WHITE = (0.07, 0.11, 0.22), (0.45, 0.70, 1.0), (1, 1, 1)


def deck(out):
    doc = pymupdf.open()
    W, H = 960, 540

    def slide(dark=False):
        page = doc.new_page(width=W, height=H)
        page.draw_rect((0, 0, W, H), color=None, fill=NAVY if dark else (0.985, 0.988, 0.995))
        return page

    p = slide(dark=True)
    page_dots(p, W, H)
    text(p, (80, 190, W - 80, 260), "Field Study: Pens in the Classroom", SANS_B, 38, WHITE)
    text(p, (80, 255, W - 80, 290), "What we learned from twelve weeks of handwritten notes", SANS, 18, SKY)
    text(p, (80, 440, W - 80, 470), "Design review  \u00b7  November", SANS, 13, (0.7, 0.75, 0.85))

    p = slide()
    heading(p, "Three findings", W)
    for i, (h, b) in enumerate([("Ink must keep up", "Students stopped noticing the screen when strokes followed the tip."),
                                ("Fewer buttons", "A folded toolbar left two thirds more of the page visible on a phone."),
                                ("Marks are kept", "Highlights and notes opened unchanged in every other PDF reader.")]):
        x = 80 + i * 280
        p.draw_rect((x, 170, x + 250, 400), color=None, fill=WHITE, radius=0.06)
        p.draw_rect((x, 170, x + 250, 400), color=RULE, width=0.8, radius=0.06)
        p.draw_circle((x + 40, 214), 18, color=None, fill=SKY if i != 1 else (0.36, 0.78, 0.6))
        text(p, (x + 31, 202, x + 49, 228), str(i + 1), SANS_B, 16, WHITE, align=1)
        text(p, (x + 24, 250, x + 230, 280), h, SANS_SB, 18, NAVY)
        text(p, (x + 24, 285, x + 230, 390), b, SANS, 13, MUTED, lh=1.4)

    p = slide()
    heading(p, "Time spent annotating per lecture", W)
    vals = [("Week 1", 4), ("Week 3", 7), ("Week 5", 9), ("Week 7", 12), ("Week 9", 14), ("Week 12", 17)]
    base, top = 430, 170
    for i, (lbl, v) in enumerate(vals):
        x = 120 + i * 125
        h = (base - top) * v / 18
        p.draw_rect((x, base - h, x + 70, base), color=None, fill=SKY if i < 5 else (0.16, 0.38, 0.80), radius=0.08)
        text(p, (x - 10, base + 8, x + 80, base + 28), lbl, SANS, 12, MUTED, align=1)
        text(p, (x - 10, base - h - 24, x + 80, base - h - 4), f"{v} min", SANS_SB, 12, NAVY, align=1)
    p.draw_line((100, base), (W - 80, base), color=RULE, width=1)

    p = slide(dark=True)
    page_dots(p, W, H)
    text(p, (80, 215, W - 80, 280), "Thank you", SANS_B, 44, WHITE, align=1)
    text(p, (80, 285, W - 80, 320), "Questions and comments welcome", SANS, 18, SKY, align=1)
    doc.save(out, garbage=3, deflate=True)


def heading(p, s, W):
    text(p, (80, 70, W - 80, 120), s, SANS_B, 30, NAVY)
    p.draw_line((80, 125), (160, 125), color=SKY, width=4)


def page_dots(p, W, H):
    for i in range(14):
        for j in range(4):
            p.draw_circle((W - 60 - i * 22, 40 + j * 22), 2.2, color=None, fill=(0.2, 0.28, 0.45))


# ---- The form
def form(out):
    doc = pymupdf.open()
    W, H = pymupdf.paper_size("letter")
    M = 72
    page = doc.new_page(width=W, height=H)
    page.draw_rect((0, 0, W, 120), color=None, fill=NAVY)
    text(page, (M, 44, W - M, 80), "Workshop Registration", SANS_B, 24, WHITE)
    text(page, (M, 80, W - M, 100), "Digital Ink in Teaching  \u00b7  Spring term", SANS, 11, SKY)
    y = 160
    text(page, (M, y, W - M, y + 40), "Please fill in this form and bring it on the first day. All fields can be changed later.", SERIF, 10.5, MUTED)
    y += 46

    def label(s, yy):
        text(page, (M, yy + 4, M + 120, yy + 22), s, SANS_SB, 10.5, INK)

    def widget(kind, name, rect, **kw):
        w = pymupdf.Widget()
        w.field_type, w.field_name, w.rect = kind, name, pymupdf.Rect(rect)
        w.border_color, w.border_width, w.fill_color = RULE, 1, (0.97, 0.98, 1.0)
        w.text_font, w.text_fontsize, w.text_color = "Helv", 11, INK
        for k, v in kw.items():
            setattr(w, k, v)
        page.add_widget(w)

    label("Full name", y); widget(pymupdf.PDF_WIDGET_TYPE_TEXT, "name", (M + 130, y, W - M, y + 24)); y += 44
    label("Email", y); widget(pymupdf.PDF_WIDGET_TYPE_TEXT, "email", (M + 130, y, W - M, y + 24)); y += 44
    label("Session", y)
    for i, opt in enumerate(["Morning", "Afternoon"]):
        x = M + 130 + i * 130
        widget(pymupdf.PDF_WIDGET_TYPE_RADIOBUTTON, "session", (x, y + 4, x + 16, y + 20), field_value=opt)
        text(page, (x + 24, y + 3, x + 120, y + 22), opt, SANS, 10.5, INK)
    y += 44
    label("Track", y)
    widget(pymupdf.PDF_WIDGET_TYPE_COMBOBOX, "track", (M + 130, y, M + 330, y + 24),
           choice_values=["Beginner", "Intermediate", "Advanced"], field_value="Beginner")
    y += 44
    label("Notes", y); widget(pymupdf.PDF_WIDGET_TYPE_TEXT, "notes", (M + 130, y, W - M, y + 80), field_flags=4096); y += 104
    widget(pymupdf.PDF_WIDGET_TYPE_CHECKBOX, "agree", (M + 130, y + 2, M + 146, y + 18), field_value=False)
    text(page, (M + 156, y + 1, W - M, y + 22), "I agree to the workshop's code of conduct", SANS, 10.5, INK)
    # (PyMuPDF makes a radio button "on" in its own name; neither is chosen until the person chooses.)
    for w in page.widgets():
        if w.field_type == pymupdf.PDF_WIDGET_TYPE_RADIOBUTTON:
            doc.xref_set_key(w.xref, "AS", "/Off")
            kind, parent = doc.xref_get_key(w.xref, "Parent")
            if kind == "xref":
                doc.xref_set_key(int(parent.split()[0]), "V", "/Off")
    doc.save(out, garbage=3, deflate=True)


if __name__ == "__main__":
    folder = sys.argv[1]
    os.makedirs(folder, exist_ok=True)
    paper(os.path.join(folder, "Writing on PDFs with a Pen.pdf"))
    deck(os.path.join(folder, "Field Study.pdf"))
    form(os.path.join(folder, "Registration.pdf"))
    print("documents in", folder)
