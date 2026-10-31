# How input works (and how it will)

The plan for **Milestone 1.4.0** (`ROADMAP.md`, tasks I1 to I6). Part A says what the program does today, with the files; part B is the
design to build. Written 2026-10-30 with the owner; **status (2026-10-30): I1 to I6 are built** (`input/rules.rs` the table, `input/click.rs` the click limits, `editor/cursor.rs` and `editor/reader.rs`, the shell's `sync_cursor`, Android's pointer icons through `Host.setPointer`), except the custom hardware cursor images for the pen's dot and the eraser's ring (a crosshair stands in for both) and the Shift+click extend; B1 to B9 otherwise describe what the program does.

**Goals, in order:** (1) drawing latency does not move; (2) a pen, a mouse and a finger each do what a person expects of a PDF reader;
(3) the cursor always says what the next press will do; (4) one table is the truth, so the cursor, the press and the tests cannot disagree;
(5) the input promise stays: every tool works with only the left mouse button and the keyboard (desktop) or only the stylus tip (tablet).

---

## A. Today

### A1. The path of an event

```
 OS pen/touch reader ──► PointerEvent ─┐                                (platform/*, drained once per frame: PointerSource)
 winit mouse, keys, wheel ──► InputEvent ┴─► Editor::handle / pointer_event / mouse_* / wheel / key / shortcut   (editor/dispatch.rs)
     1. modal gate        editor/modal.rs      the prompt, or the pages overview, takes everything (left button only for the mouse)
     2. palm guard        input/palm.rs        a touch is dropped while a pen is near, or if its contact is bigger than 106 logical px
     3. chrome            editor/toolbar_ui, pills, context, tool_panel, search, theme_menu     a press on a button/card/slider is the window's, not the page's
     4. page              touch -> TouchGestures (pan, pinch, fling, tap)   editor/touch.rs
                          pen / mouse -> route_to_tool -> the tool's policy -> Tool::pointer    editor/routing.rs
```

`InputPolicy { stylus, mouse: Scrolls | Itself | AsStylus, keyboard }` (tools/mod.rs) is each tool's answer to "which devices do you listen to
in this mode". A mouse that stands in for a stylus is handed to the tool as a `Device::Pen` with a fixed id, so no tool knows the difference:
left button = the tip, **right = the eraser end, middle = the barrel button** (`editor/mouse.rs: stylus_for`).

### A2. Devices and modes today

| | Desktop mode | Tablet mode |
|---|---|---|
| **Mode is set by** | `--mode`, the mode button, else the OS: slate posture on a machine with touch or a pen digitiser means tablet | same |
| **Mouse** | Pen, Shape, Select, Lasso, Eraser, Laser: as a stylus. Text: as itself (click places a box). | scrolls only |
| **Stylus** | all tools but Text (Text: the stylus does nothing in desktop mode) | all tools |
| **Finger** | pan, pinch-zoom, fling, tap (a tap follows a link; on empty page clears the selection) | same |
| **Keyboard** | shortcuts (Ctrl+Z/Y/Shift+Z, S, Shift+S, A, C, X, V, D, F) and the selection's keys (Delete, Esc, arrows, Backspace on a shape point) | typing only, no shortcuts |
| **Wheel** | scrolls (lines, or exact pixels for a touchpad); never zooms | same |

Pen buttons: the **tip** draws (with the Lasso tool it loops, with the Eraser tool it erases), the **eraser end** erases and the **barrel
button** loops, with any tool that makes items, each on its own kind of item (`Workspace::role`). On Android an S Pen's one button erases.

### A3. What is missing or wrong today

1. **No cursor is ever set** (the shell never calls `set_cursor`): the system arrow everywhere, nothing says a link, text or tool.
2. **Two click paths:** a finger's tap is in `editor/touch.rs` (`TouchGestures`), a mouse or stylus click on a link is `watch_link_click` in
   `editor/navigation.rs`. Same idea, different constants and different code.
3. **No reader keys or mouse zoom on a desktop:** no Ctrl+wheel, no zoom keys, no Page Up/Down, Home/End or arrows to scroll when nothing is
   selected. A mouse-only person cannot zoom at all (the input promise says every tool must work with left button and keyboard).
4. **Empty-page drag with the Select tool does nothing** (a reader's drag pans); a drag on a link does nothing special.
5. **No double tap.** The page opens fitted to the width and a pinch leaves it, with no quick way back.
6. The device-rule test (`editor/tests/contract.rs`) has no Lasso or Eraser rows.

---

## B. The design

### B1. One function decides: `decide`

```
 Affordance = decide( layer, mode, tool, device, role, target, modifiers )
```

It is a pure function in the domain layer (`input/rules.rs`), no window, no GPU. **Both the press and the cursor call it.** The press runs it
once, when the pointer goes down. The cursor runs it when the pointer hovers (mouse, or a hovering stylus). A finger has no hover.

| Input to `decide` | Values |
|---|---|
| **layer** | `Modal` (prompt, overview), `Chrome` (toolbar, bar, panel, cards), `Gesture` (a press already under way), `Page` |
| **mode** | `Desktop`, `Tablet` |
| **tool** | Select, Pen, Geometry, Text, Lasso, Eraser, an extension (by name) |
| **device** | `Finger`, `Stylus`, `Mouse` |
| **role** | what the device's buttons make of the press: `Draw` (the tip, the left button), `Erase` (the eraser end, the right button), `Loop` (the barrel button, the middle button) |
| **target** | `Ink` / `Shape` / `TextBox` (an item of ours), `Markup`, `Handle` (a selection or shape handle), `TextRun` (a PDF letter), `Link`, `Empty` (page paper), `Off` (outside any page) |
| **modifiers** | Ctrl, Shift, Alt (and a held Space is **not** used) |

It returns an **`Affordance`**: `{ action, cursor }`.

- `action` is what the press will begin: `Ink`, `ShapeDraft`, `PlaceText`, `EditText`, `LoopSelect`, `Erase`, `Grab`, `SelectText`,
  `FollowLink`, `Pan`, `Zoom`, `PressButton`, `Nothing`, and so on.
- `cursor` is a `CursorKind` (B7).

**The table is the truth.** It is one `match` (or a `const` table), not rules scattered across tools. A test enumerates every combination and
asserts it has exactly one answer, so "which rule wins" cannot arise. The tools' `InputPolicy` is derived from it, not a second copy.

### B2. Layers and capture (who owns the pointer)

From strongest to weakest, and the first that applies wins:

1. **Modal**: the unsaved-changes prompt, the slides (presentation mode: a click, tap or swipe turns the page, Escape leaves), the pages overview. Nothing reaches the page.
2. **Chrome**: toolbar, the bar under the tools, the tool panel, the search card, the theme card. A press that **begins** on chrome is the
   chrome's until it lifts, even if the pointer leaves it (sliders, a pressed button). The toolbar is reachable by every device.
3. **Gesture under way**: a press that began on the page keeps the pointer until release. Moves and the release go to the tool that has it
   (`Tool::owns`), whatever is under the pointer now. A second device never takes it over.
4. **Page**: `decide` on the target under the pointer.

A press that starts a gesture decides **once**. Nothing about a drag is re-decided as it moves.

### B3. Click, drag, double tap: one recogniser for every device

Replaces `TouchGestures`' tap path and `watch_link_click`. It is a small state machine fed `Down`, `Move`, `Up`, `Cancel`:

```
click   iff   ‖x_up − x_down‖ ≤ slop · dpr    and    t_up − t_down ≤ T
```

| Constant | Finger | Stylus / mouse | Why |
|---|---|---|---|
| `slop` (logical px) | 12 | 6 | a fingertip wanders; Android's own touch slop is 8 dp and a tablet finger more |
| `T` | 350 ms | 800 ms | a slow finger is a pan or a press-and-hold; a mouse click is slower and exact |
| double: gap | 300 ms | 450 ms (a word/line click, as `select.rs` has) | |
| double: distance | 2 · slop | 12 px | |

All in one file, all in **logical** pixels multiplied by `dpr` (the 1.3.3 bug was screen pixels). The same recogniser serves: following a
link, placing the caret, double click a word, triple click a line, double tap fit-width, tap on empty page to clear.

**What a drag is** is whatever is not a click, and what it does comes from `decide` at press time (B5).

### B4. Roles: buttons are held modifiers

A button of the pen or the mouse that is not the tip is a **quasimode** (Raskin): it applies only while held and cannot be left on.

| | Draw | Erase | Loop |
|---|---|---|---|
| **Stylus** | tip | eraser end | barrel button |
| **Mouse** | left | right | middle |
| **Finger** | never draws | | |

With the **Lasso** tool the Draw role means Loop; with the **Eraser** tool it means Erase (`Workspace::role`, already so). The eraser end and
the barrel button keep their meaning in every tool that makes items. In the **Select** tool (the reader) **the middle button pans**, since a
reader has nothing to loop; in a drawing tool it stays the barrel (loop). Right-click with the mouse is the eraser end in the drawing tools and
does nothing in the reader (no context menu yet).

### B5. The Select tool is the reader

It is the default tool and the one that must feel like a PDF viewer. Targets and what a press does (mouse and stylus; the finger rows are in B6):

| Target under the pointer | Cursor | Click | Drag |
|---|---|---|---|
| `Link` | pointing hand | follows it | if it began on text: selects text; else pans |
| `TextRun` | I-beam | places the selection anchor; double click a word; triple click a line; Shift+click extends | selects in reading order |
| `Markup` (a highlight etc. made here) | arrow | selects it (to move or delete) | moves it |
| `Handle` of the text selection | resize | none | stretches the selection |
| `Empty` page, or `Off` the pages | arrow | clears the selection | **pans** (closed hand while dragging) |
| chrome | arrow | the button | none (the button's own drag) |

Keyboard in the reader (desktop; none of these steal a key from typing in a text box or the search field):

| Key | Does |
|---|---|
| Ctrl+A / Ctrl+C | select the page's text / copy it (exists) |
| Esc | clear the selection (exists) |
| Ctrl+F | the search card (exists) |
| **new** Ctrl+wheel, Ctrl+`+` / Ctrl+`-` | zoom about the pointer (about the window's centre for the keys) |
| **new** Ctrl+0 | fit width; Ctrl+1: 100% |
| **new** PageUp / PageDown / Space / Shift+Space | one screen down / up |
| **new** Home / End, Ctrl+Home / Ctrl+End | top / bottom of the page / of the document |
| **new** arrows (nothing selected) | scroll a little (with a selection they keep nudging it) |

### B6. The finger (every tool, both modes)

A finger never draws; it moves the view.

| Gesture | Does |
|---|---|
| one finger drag | pan, with a fling on release (speed threshold in logical px/s) |
| two fingers | pinch-zoom and pan about their centre |
| tap on a link | follows it. **Instantly**: no wait for a second tap |
| tap on a box of a form field | fills it (a form box is a link of `Target::Field`: same click, same devices): a check box turns, a radio button is chosen, a choice goes to its next option, a text field takes the keyboard |
| tap on a toolbar or card button | presses it |
| tap on empty page | clears the selection. Instant, no other effect |
| **double tap on the page (not on a link)** | **fit the page to the width**; a second double tap returns to the zoom and scroll it came from. Any other zoom or pan forgets the remembered view |
| pulling past the last page | adds a page (exists) |

Why the double tap costs nothing: a single tap on empty page has no action but clearing the selection, which is done at once; the second tap
of a double tap just also fits the width. A tap on a link never waits to see whether another follows.

Palm rule (exists, `input/palm.rs`): touch is dropped while a pen is near, and a contact bigger than 106 logical px (28 mm) is a palm.

### B7. Tools, one row each (stylus tip / mouse in the desktop mode)

| Tool | Press | Drag | Hover cursor | Notes |
|---|---|---|---|---|
| **Select** | B5 | B5 | per target (B5) | the default; reads, never draws |
| **Pen** | starts a stroke | the stroke | dot / small crosshair | a link or text under it is **not** read; ink wins |
| **Shapes** | starts a draft (or taps a shape to pick it up) | the draft | crosshair | |
| **Text** | places a box / edits the one under it | moves it | I-beam over a box, crosshair elsewhere | desktop: the mouse is itself, not a stylus |
| **Lasso** | a tap picks the item under it; on the selection it grabs it | a loop; or moves the grabbed selection | crosshair; move cursor over the selection | any kind of item |
| **Eraser** | erases at once under the tip | erases what it passes | ring of the eraser's size | any kind of item |
| **Laser** (extension) | the pointer | a fading trail | dot | |
| any other extension | its own | its own | `Crosshair` unless it says (B9) | |

**Conflict rule (the one that decides most cases):** *ink cannot wait*. A drawing tool must draw under the tip within a frame, so it acts on
the press and **cannot** wait to learn whether the press was a link click or the start of a text selection. Reading can wait until release or
until the pointer moves. So **a drawing tool never loses a press to a link or to text**; to follow a link or select text, choose the Select
tool. The hardware shortcuts (barrel button, eraser end) are the only things that act inside a drawing tool without changing it.

### B8. Modes and what triggers them

| Mode | Triggered by | Changes |
|---|---|---|
| **Desktop** | the OS (not a slate), `--mode desktop`, or the mode button | the mouse acts (as a stylus, or as itself in Text); shortcuts work |
| **Tablet** | the OS (slate posture with touch or a pen digitiser), `--mode tablet`, or the mode button | the mouse only scrolls; no shortcuts; typing only into a text box |

- **Only these** switch the mode. The pen touching the screen does not (no automatic switching), so a mouse and a pen can both be used on a
  laptop in desktop mode.
- **Tool changes** come from the toolbar buttons (every device can press them) and, in desktop mode, from nothing else yet (no letter keys;
  they would steal typing).
- **The finger button** (right pill, saved) is not a mode either: with it on, one finger on the page is the stylus tip for the tool in use, under the
  tablet mode's rules (the tip alone does everything), in either mode, so that a person with no stylus has every tool. A stroke is begun when the finger has
  moved 8 logical px or lifts, so a second finger that comes down with it (a pinch) draws nothing; a second finger later ends the stroke, and two fingers pan
  and zoom. Chrome (toolbars, bar, cards, overview) is pressed by a finger as always, and the palm rule below still applies.
- **Pen proximity** is not a mode but a filter: while a pen hovers or touches, finger touches are dropped (palm rule).
- **Held modifiers** (B4) are the only transient modes.

### B9. Cursors

A `CursorKind` is a small enum the editor reports; the shell turns it into a system cursor. The editor computes it from `decide` **only on a
hover or mouse move, never while a press is held and never on the ink path**, and the shell calls `set_cursor` **only when the kind changes**.

| `CursorKind` | When | Windows / Linux (winit `CursorIcon`) | Android (pointer icon, mouse/stylus hover only) |
|---|---|---|---|
| `Arrow` | chrome, empty page, off pages | Default | TYPE_ARROW |
| `Hand` | a link in the reader | Pointer | TYPE_HAND |
| `Text` | a text run in the reader; a text box with the Text tool | Text | TYPE_TEXT |
| `Crosshair` | drawing tools with no better one | Crosshair | TYPE_CROSSHAIR |
| `Dot` | Pen, Laser | custom image (a small dot) | custom or crosshair |
| `Eraser` | Eraser tool | custom image (a ring at the eraser's size) | custom or crosshair |
| `Move` | over a selection with the Lasso tool | Move | TYPE_ALL_SCROLL |
| `Resize` | a selection handle | ColResize / NwseResize | TYPE_GRAB |
| `Grab` / `Grabbing` | empty page in the reader / while panning | Grab / Grabbing | TYPE_GRAB / TYPE_GRABBING |
| `None` | while a stylus presses in a drawing tool and the tool draws its own tip | hidden | hidden |

Extensions get an optional `cursor(ctx, pos) -> Option<CursorKind>` in `inkpdf-api` (added, not changed, so extensions keep working).
A custom cursor is a **hardware cursor image** composited by the OS: it costs the frame nothing. Never a software cursor during a stroke.

### B10. Latency guardrails (acceptance criteria)

The pen path today is `input → app → GPU → scan-out` (`platform/mod.rs`). This milestone adds **nothing to it**.

1. `decide` is not called per stylus or mouse move while a press is held. A gesture is decided once, at `Down`.
2. The pen's move path allocates nothing and reads no table.
3. `set_cursor` is called on a change of kind only, from hover, off the pen path.
4. Hover hit tests are cheap and never block: links come from the visible page's in-memory list; text runs only from the page text already
   indexed (not ready yet: the arrow, nothing waits); items of ours from the existing spatial lookup.
5. No software cursor while drawing; custom cursors are OS-composited images.
6. A test in the latency check CI already runs proves (1) and (2), and the numbers of `ROADMAP.md` section 1 do not move.

### B11. Tests

- **The exhaustive table test** (I1): every `(layer, mode, tool, device, role, target)` has exactly one row; every tool is covered.
- **Capability matrix** (exists, `editor/tests/capability.rs`) extended with the reader rows and the keys of B5.
- **Cursor tests** (I3, I5): a recording fake platform captures `set_cursor` calls; for each row of B5, B7, B9 the hover gives the cursor.
- **Click recogniser** (I2): the same click gives the same result on a finger, a mouse and a stylus; the 15 px wobble follows a link at
  dpr 2.6 and does not at dpr 1; a double tap toggles fit-width and back; a tap on a link never waits; a drag from empty page pans.
- **Replay**: recordings (`--record` / `--replay`) of a reading session and a drawing session replay to the same state.
- **Storms** (`stress/`) keep running against the table.

### B12. Not included, by decision

- Space+drag to pan (the middle button and the finger already pan, and Space is a key the text tool needs).
- A rubber-band selection on empty page in the reader (the Lasso tool picks up items; a drag on empty page pans).
- Automatic desktop/tablet switching on pen contact.
- Double tap to zoom to a point (only fit width and back).
- A context menu on the right button.
- Letter shortcuts to change tool.

### B13. Open points to settle while building

- Whether PageUp/PageDown/Space also work in the drawing tools (they should, as they cannot be typing unless a text box is open).
- The exact custom cursor images (a dot, a ring) and their hot spots; whether Linux Wayland compositors honour custom images (fall back to
  `Crosshair` where they do not).
- What the reader's right button should do when a context menu exists (copy, search the selection).

---

## C. Build order

See `ROADMAP.md`, Milestone 1.4.0: **I1** the table and its test; **I2** the click recogniser and double tap; **I3** cursors on Windows and
Linux; **I4** Android pointer icons; **I5** the reader rules and keys (B5, the keyboard rows); **I6** the latency guard test, docs, release 1.4.0.
