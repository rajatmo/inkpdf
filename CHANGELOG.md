# Changelog

## 1.3.0

Extensions: a tool can be added without touching the program; Android files; polish.

- The tools are a registry (`tool::Registry`) instead of a fixed list: the toolbar, the mark that slides between tools, the settings and
  the keyboard read it, so a tool is a name and an object, and the toolbar has as many buttons as there are tools (up to eight).
- New crates: `inkpdf-api` (the extension API: plain types and traits, no dependencies) and `inkpdf-ext` (the built-in extensions, one folder
  each, found by a build script, so adding one edits no existing file). The program adapts an extension's tool to its own (`tools/ext.rs`).
- **The laser pointer is now an extension**, written against the API alone (its tests are in its own crate); it works as before.
- `tools/new-extension.sh` makes a new extension from a template; `docs/EXTENSIONS.md` says how. Tests: a tool registered from outside is on
  the toolbar, takes input, paints and keeps the frames coming; every extension a build has is checked (name, API version, icon); the contract
  suite and a latency cap run on every extension by themselves; an extension may depend on nothing but the API (a reviewed list) and touches
  no file, network or process.
- Inside: the tests of the toolbar and the interface are written against a reference set of tools, so that adding an extension changes none.
- **Android files, clipboard and links** (written, not yet tried on a device): the APK now has a small Java activity (`Host`) beside the Rust library. A folder button on the toolbar (where the system has a picker) opens a PDF through the system's file picker; Save As uses the system's "create document" screen and writes to the address it hands out; "Open with InkPDF" opens a PDF from a file manager; copy and paste use the system clipboard; links open in the browser. Inside: `platform/saf.rs` (the logic behind a `Bridge`, tested with a fake), `platform/android/host.rs` (JNI), `DocId::Uri` now used.

- The bar under the tools now goes out as it came in (it slides up and fades) when you choose a tool that has none (the pointer), where it
  used to vanish; with animations off it is gone at once, as before.
- In the pages overview a picture that arrives after the overview was opened fades in over a moment instead of appearing; one that is
  already there is simply there.
- Inside: `stress.rs` is now `stress/` (the storms in files by topic: editor, ink, files, shapes, pages, overview) and
  `editor/overview.rs` is `editor/overview/` (state, input, actions); no change in behaviour (the same 800 tests).

## 1.2.0

Text: select it, copy it, mark it, find it.

- **Select text.** A new tool (the I-beam, between the text tool and the pointer) selects the text a PDF has: drag over it, press twice
  for a word, three times for a line, move a selection by the round handle at each end (the only way to stretch it with a stylus tip on a
  tablet), Ctrl+A for the page, Ctrl+C or the copy button to copy (Esc lets go). The bar under the tools then has copy, delete and three
  marks: **highlight, underline and strike-out** make a markup of the lines selected. A markup is selected by pressing on it, can be moved,
  deleted, erased and undone like ink, and is saved as a standard Highlight, Underline or StrikeOut annotation, so any reader shows it.
  Annotations other programs made are still left alone. (The squiggly kind is in the model and the file but has no button yet.)
- **Search.** The magnifier at the left of the toolbar, or Ctrl+F, opens a card at the right: type, and every place the words are found
  is lit on the page (the current one stronger), with a list of them (a line of text round each, press one to go to it), "3 of 17", next
  and previous (the arrows; Enter and the Up and Down keys too), and the options case and whole words (remembered). It searches the text
  of the pages as they are read (the card says how many are done) and the text boxes you typed. Works with the left mouse button and
  keyboard, or the stylus tip alone.
- The text of the pages is read on a thread of its own (`indexer.rs`), nearest pages first, so a long document is searchable as it opens
  without holding up a frame.
- Inside: `TextSource` joins the PDF seam (the file stays parsed between pages); `doctext.rs` keeps the text by page of the file, as the
  pictures are kept, so page edits never read a page again; text markups are strokes of boxes in the ink store (`Stroke::markup`), through
  the same undo, selection, page-edit and save code as every other stroke; `tools/select.rs`, `editor/search.rs`, `toolbar/search_card.rs`;
  `View::scroll_to_show`; the interface can now have text (the search card's), drawn from bitmaps as the overview's is. New tests: a fake
  page of text on a grid, the indexer, the markup in the store and in the file (also on a turned page), the select tool and the search card
  with every device, the storms with text on every page, and real-window checks of both in `tools/e2e/linux-smoke.sh`.
- The toolbar's room: five tools and a magnifier made it a little wider; the buttons shrink earlier on a narrow window.

## 1.1.0

A dark look, and the PDF engine behind a seam of its own.

- **Dark theme.** The interface has a light and a dark look. By default it follows the system's (Windows, and on Linux the desktop's
  `color-scheme`; on Android it is light for now, because the system's setting cannot be read yet); the new button at the top right (a
  half-shaded circle, a sun or a moon) goes from "system" to "light" to "dark" and round, on every device, and is remembered. The
  colours fade from one look to the other (at once when animations are off), and on Windows the title bar follows. `INKPDF_THEME=light|dark`
  chooses the look for one run.
- **Dark pages.** The button beside it (a page with part of it in shade) draws the pages themselves on a dark paper with light text
  when the look is dark. Photographs keep their colours. Ink you drew in dark colours is shown turned over (black ink shows white), and
  typed text too; the file is never changed, so what is saved is exactly what you drew, in the colours you chose.
- Settings gain `appearance` and `dark-pages` lines; a settings file from before 1.1 reads as "system, pages as they are".
- Inside: every colour is a role of a `Palette` (`palette.rs`, with the light palette's values exactly the old ones, checked by a
  golden test of everything drawn, plus contrast checks for both looks and a test that no colour literal creeps back into `toolbar/`
  or `tools/`); the editor resolves the look from the user's choice and the system's; the PDF seam takes a `PageLook` and the pdfcraft
  side maps it to reading colours (a pool is rebuilt when it changes). New tests: the palette, the look through settings, the fade,
  both buttons with the mouse and with the stylus, the ink mapping, a dark render through the conformance suite, saved files unchanged by the look,
  and a pixel check of the dark look in `tools/e2e/linux-smoke.sh`.
- Inside: the PDF engine sits behind one seam (`pdf::PdfBackend`), so the program speaks its own types and only `pdf/pdfcraft.rs` names pdfcraft
  (a test says so); the editor takes its motion preference explicitly (no more `cfg!(test)` in the program).
- **InkPDF runs on Linux and on Android**, beside Windows, from the same code. Every operating system gets its own small pen reader behind
  one `Platform` object (Windows: pointer messages with their history; Android: the activity's `MotionEvent`s, so stylus, finger, eraser,
  buttons and hover are told apart; Linux: winit's basic input for now), and everything above it is shared. Windows behaves as before.
  Packaging: a Linux portable tarball (`tools/linux/build-portable.sh`), an Android APK (`tools/android`), and CI jobs that build both
  and the Windows portable. Android is a test build: no file picker or clipboard yet.
- The Android reader lets go of touches the system cancels (a screen cancels its touches when the pen comes near; they used to stay
  down, so the next one-finger pan zoomed), and tells a pen and a finger with the same pointer number apart.
- The README is for users first (where it runs, how to get it, how to use it), with the developer notes after.
- On Android the stylus's first button (the S Pen's one button) erases, as in Samsung Notes; a second button is the barrel button (lasso), and a
  real eraser end erases as before. Windows and Linux are unchanged: the eraser end erases and the barrel button lassos.
- `--record FILE` saves the input the program was given (pen, touch, mouse, keys) as text, and `--replay FILE` plays it back at its
  old pace and closes: the same recording gives the same document on every system, so a problem seen on one machine can be played
  on another, and the speed of the editor can be measured with no one at the screen.
- The pen's events are handed to the editor without allocating each frame.
- Text boxes are set in Arial where Windows has it and in a built-in copy of Liberation Sans (as wide as Arial) everywhere else,
  so a text box takes the same room on every machine. Settings are kept in the platform's configuration folder (on Android, the
  app's private one), through the platform layer.
- Palm rejection is one rule: touch is off for as long as the pen is above the screen or touching it, and comes back the moment the pen
  leaves (no waiting period). Fingers already down when the pen arrives are given up cleanly (before, they could stay stuck as a
  phantom finger). A pen whose screen does not report hover no longer locks touch out after a stroke. A contact too big for a
  fingertip is still taken for a palm.
- Save As is the app's own card, on every platform (it replaces the system's save window, which stays hidden when raised from a
  pen tap and looks different everywhere): it opens on the Documents folder (the one Windows names, which may be in OneDrive;
  on Linux the desktop's own; the document's folder when it already has a file), offers `Untitled.pdf` with its name selected,
  and has places to start from, the folder you are in with an up button, its subfolders and PDFs, and Cancel and Save. A new
  document saves through it from Ctrl+S or the save button; Ctrl+Shift+S saves any document under another name.
- Every device finishes it: the left mouse button and keyboard at a desk, the stylus's tip on a slate (the name offered can be
  saved as it is, so no keyboard is needed), a finger taps and drags the list, the wheel scrolls it. A name is checked
  (no `/ \ : * ? " < > |`, no `CON`/`NUL`, no trailing dot, `.pdf` added) so a file saved here opens anywhere; a file that
  is there is replaced only when Save is pressed a second time.
- Inside: `browse.rs` (the entries, the places and the file-name rules, no file system), `editor/save_as.rs` (the state, which
  asks the shell to list a folder and to write the file through `Request`s), `toolbar/save_as.rs` (layout and drawing as plain
  data), `app/folders.rs` (folders are read on a thread of their own, so a slow drive cannot stall a frame). The `rfd`
  dependency is gone. New tests: the name rules, the dialog's state machine, every device through the editor, and a hostile storm.

## 1.0.0

Motion, the Material way: the interface now moves instead of switching.

- Buttons: the wash under the pointer fades in and out, a press spreads from the middle of the button, the mark of the tool
  in use slides from one tool to the next (its icon changes colour as it passes), and undo, redo, delete page and save fade
  between their states.
- The bar under the tools comes down into place; the settings panel unfolds from it (the card grows, then the contents
  appear) and folds up again; the "save your changes?" prompt grows in over a clearing scrim and goes out as it was.
- The pages overview fades in and goes out; after a page is added, copied, moved or deleted the pages slide to their new
  places and a new one grows in; a page you choose pops its tick; the wheel and the keys glide the pages.
- Less motion on request: it follows Windows' "Show animations" setting (and `INKPDF_MOTION=off` or `on` for one run). With it
  off nothing takes any time at all. An idle window still costs no CPU, and the pen's latency is as it was.
- Inside: one small module of curves and tweens (`motion.rs`), one registry that owns every animation
  (`editor/motions.rs`), and transforms put over groups of what is drawn (`scene::Xform`, `Ui::apply`); the drawing code
  receives plain numbers. New tests: curves and tweens, each animation, the equivalence of motion on and off, and a hostile
  storm with motion on.

## 0.7.0

Pages overview (the candidate for 1.0): a card over the document, opened from the new pages button at the end of the left
toolbar, with a picture of every page (the ink drawn on it), its number, and the PDF's table of contents on the left when it
has one. Tap a page or an entry to go there. The check mark is edit mode: choose pages, then duplicate, move earlier or
later, or delete them from the bar under the pages; drag a page (or all the chosen ones) to where it should go; the number
under each page has a menu (add before, add after, duplicate, delete). Every device works: the mouse, the stylus tip, a
finger (which scrolls, and uses the bar to move pages), and at a desk Ctrl+A, Ctrl+D, Delete and Esc.

- Pages are a step of the history now: add, delete, duplicate and move all undo and redo, and undoing a deletion brings back
  the page with its ink, its links and the outline entries that went to it (until the file is saved, when the page is
  gone from the file). Before, deleting a page could not be undone and dropped the whole history.
- Saving writes pages in their new order, copies pages from the page they copy (so a copy survives the deletion of its
  original), and the table of contents follows (entries of a deleted page lose their destination).
- Inside: a page edit is one value (`PageEdit`) that the ink, links, outline, selection, layout and the file's page table
  all take (`pages.rs`), replacing six hand-kept copies of the same arithmetic; the editor does every page edit
  (`editor/pages.rs`) and tells the shell once (`Request::PagesEdited`); the layout is no longer kept twice. The UI can
  now draw text and small pictures of pages (a layer between the interface's panels and lines), and the ink in them.
  A new storm drives the overview with hostile input.
- The overview is smoother: the wheel and the keys glide the pages and the table of contents to where they go (a finger
  and a dragged page still follow at once), the card fades in, and in edit mode a page that is not chosen has an open
  ring to tick in its top right corner (it was a white circle on white paper, laid over the page's title).
- The pictures of the pages in the overview show what is on the pages: typed text (before, only strokes and shapes),
  the strokes that were chosen when it was opened (they were left out), and everything on a page that was added or
  whose picture is still being made (before, those showed nothing at all: an empty page however much was drawn on it).
- Quicker and lighter: pictures of pages are kept under the page of the file that draws them, so adding, deleting,
  moving or undoing pages no longer throws the rendered pages away and renders them again; the table of contents rows
  are built once, not on every pointer move; the pages kept for deletions go as soon as the history lets go of them
  (it keeps 500 steps), not only when the file is saved; the bitmaps of the numbers in the overview are trimmed. A
  30-round soak of real clicks and wheel turns (ink, overview, add, delete, duplicate, undo) holds memory, handles and
  threads flat once the picture cache is full (its budget is 768 MB).
- Damaged files: an outline or links that point at pages the file does not have are dropped when the file is opened,
  instead of being kept until they cause trouble. A storm opens files with hostile outlines and page trees.
- A shape's axes no longer go wrong when an axis has no length (found by a storm).
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
