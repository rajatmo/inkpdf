# Changelog

## Unreleased

- The line being drawn no longer swings: the short stretch the pen's predicted position adds ahead of the ink was a
  straight guess, which stuck out of a turn by up to several pixels and snapped back with the next sample. It is now
  aimed along the path's heading and cut short on a tight turn (at most half a pixel off the path), and is full length
  on a straight run.

- The ends of a tap-drawn spline are no longer pointed: an open end leaves with no curvature, as in Inkscape's Spiro
  and a natural spline, instead of running straight along the chord to the next point. Closed splines and the inner
  points are as before, and splines already saved keep their shape.

## 0.6.0

Input methods: with a tool chosen, every item it made can now be created, selected, moved, restyled, duplicated,
copied, pasted, erased and deleted, and each undone, with only the devices the mode guarantees (the left mouse
button and the keyboard at a desk; the tip of the stylus as a tablet). Before, selecting and erasing needed the
stylus's barrel button or eraser end or the mouse's right or middle button, and there was no way to delete a
selection, restyle what was drawn, or copy anything.

- A bar under the tools: draw / select / erase (what the plain tip or left button does), duplicate, copy, paste,
  delete, add / remove / smooth a point of a shape, and the colour and thickness. The barrel button, eraser end and
  right and middle buttons are shortcuts for the same intents.
- The settings are in the bar: the five colours are buttons in it (one tap colours the tool and what is selected; the
  one in use opens its editor), and a settings button unfolds the panel from the bar as one card. Tapping a tool's
  button now only chooses the tool (it used to open the panel). On a window too narrow for five colours they are one dot.
- The bar sits just under the tools, not at the bottom of the window (where a resting palm lands), and the panel
  opens under it. All capsules (the toolbar's and the bar) are a little smaller: buttons 38 instead of 44 units.
- With select chosen a tap selects the item under it and a loop selects what it encloses. A tool now selects, erases
  and pastes only what it made (pen: handwriting; shape tool: shapes; text tool: text boxes).
- The colour and thickness (or text size) chosen in the panel change what is selected, as one undo step per drag.
- Desktop keyboard: Ctrl+A, C, X, V, D, Delete, Esc, the arrows (Shift: further; an item on its page stays on it); with the shape tool Backspace
  takes away the point last pressed.
- A real text editor: a caret that goes where it is pressed and that the arrows, Home and End move (Shift and a
  drag select), typing in the middle of the text, Delete, and Ctrl+A, C, X, V with the Windows clipboard. Copying a
  text box puts its words on the clipboard; words copied anywhere paste into the text tool as a new box.
- Inside: the module graph is now checked against a layer table with no exceptions and no cycles (`architecture.rs`); the
  tools share one erase / select / drag piece and can offer their own buttons to the bar; the editor's window controls are a
  `Chrome` of their own and the shell can no longer edit the document by reaching into a field. No change in behaviour.
- The capability matrix (`editor/tests/capability.rs`) states the promise per tool and mode and runs it with only the
  guaranteed devices; the hostile-event storms now also press the bar's buttons, keys and shortcuts.

## 0.5.0

- A mode button on the left of full screen: it shows the mode in use (a monitor for desktop, a tablet in the accent
  colour for tablet) and switches it. It is not saved; a posture change makes the app decide again.
- The real-input tests are cut from twenty scripts and about a hundred app starts to two scripts and four starts
  (about half a minute): they now prove only what unit tests cannot see. The checks were shown to catch a broken
  mouse mode and a broken mode button.

## 0.4.0

- A delete-page button (a bin) on the left of the + button. It deletes the page being looked at, at once, with no
  question asked; the ink on it goes with it, the later pages move up with theirs, links are carried along, and
  the file loses the page when saved. The last page there is stays (the button is dimmed). It cannot be undone,
  and it drops the undo history, which may refer to the page.
- Build output, packages and test scratch files default to folders beside the repository, not in it.
- The portable build script no longer leaves its build settings behind in the shell.

## 0.3.0

- Desktop and tablet modes, chosen from the machine's posture (`SM_CONVERTIBLESLATEMODE`, guarded so that an
  ordinary desktop PC, which reports "slate", stays a desktop) and re-checked when Windows announces a
  posture change. `--mode desktop|tablet|auto` overrides it.
- Desktop: the mouse stands in for a stylus on the pen, shape and pointer tools (left draws, right erases,
  middle lassos); Ctrl+Z, Ctrl+Y, Ctrl+Shift+Z and Ctrl+S.
- Tablet: the stylus places, edits and moves text with the text tool; the mouse only scrolls.
- The window losing focus now ends a mouse gesture instead of leaving it half done.

## 0.2.0

- The application layer was rewritten around a `Tool` trait: Pen, Shape, Text and Pointer are separate
  objects that own their gestures and paint their own overlays; the device rules are data
  (`Tool::policy`); the window's asks of the shell are a request queue. See `apps/inkpdf/ARCHITECTURE.md`.
- A layering test and a tool contract suite guard the structure.
- Long strokes cost the same per frame however long they get (the live curve is extended incrementally),
  and a frame carrying new pen samples no longer waits for tile uploads.
- Standalone portable build (`installer/build-portable.ps1`) with settings kept beside the exe.
- Behaviour is otherwise unchanged from 0.1.11.

## 0.1.x

- 0.1.11: input rules per tool (the text tool is keyboard and mouse only; the stylus does nothing to the page
  with it); line and curve tools drawn by tapping.
- 0.1.10: shapes reworked into kinds with the anchors that suit them (lines and polygons, splines,
  rectangles, ellipses, axes); straight segments stay straight.
- 0.1.8-0.1.9: text tool (typed text saved as Free Text), wrapping at the page edge.
- 0.1.5-0.1.7: shape tool and editable anchors, per-tool settings, page insertion.
- 0.1.0-0.1.4: blank page start, links, swipe-up to add a page, in-app save prompt, Explorer thumbnails,
  pen panel, pointer tool.
