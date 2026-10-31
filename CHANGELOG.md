# Changelog

## 1.7.0
- **Presentation mode.** A new button on the right pill (a slide with a play triangle on a stand, left of full screen) and Ctrl+L show the document as slides: one page at a time, whole,
  as big as the window lets it be, over black, with nothing of the program's own on it. The window is made full screen for it and put back as it was. A click
  or a tap of the tip or of a finger on the right of the window, a swipe of a finger to the left, the right and down arrows, Page Down, the space bar, Enter and a
  turn of the wheel show the next page; the left third of the window, a swipe to the right, the left and up arrows, Page Up, Backspace and the wheel back show
  the one before; Home and End go to the first and the last; Escape, Ctrl+L, or the button that shows in a corner for a moment after anything is done leaves.
  It is a modal layer like the pages overview, so it needs only the left mouse button and keyboard, or the stylus tip, and a finger does it too.
- **Files that cannot be opened say so.** A file that cannot be opened (password-protected, empty, not a PDF) left a black window with no message when it was
  the first one; the error card now shows, and the program is usable (open another file) behind it.
- **A PDF's permissions are kept.** A PDF whose owner has not allowed changes (or whose copy or page edits are not allowed) is no longer annotated, saved over
  or edited anyway: the tools that change the document, the page edits and the form fields are off for it, a card says why, and what is allowed (reading,
  searching, copying where allowed, saving a copy of a form's fields where allowed) still works.
- **Two highlights are two annotations.** Two marks on one page were saved as one annotation (and read back as one, so one eraser stroke or one undo took both);
  each stroke of a mark is now its own annotation, and files saved before are read as they were.
- **Typed text in a form keeps its weight.** Text typed into a form field looked regular while typing and bold once the file was reopened: the field's font is
  now read (bold or not) and drawn the same way both times.
- **The look's pill and the bar no longer overlap.** When the window is too narrow for the bar under the tools and the right pill's drop-down cards, the bar folds
  into the round cog button (the one the look's pill shows when folded) and unfolds when it is pressed, or when there is room again; a test sweeps window sizes
  to check that nothing overlaps.
- **Android opens full screen** (the status and navigation bars hide, and come back with a swipe from the edge), and **the two settings icons differ**: the cog is
  the bar's settings button, and the look's folded pill has a new icon of its own (an eye).
- **Clean-up.** Mentions of other programs are out of the code and the documents; an empty stray file is gone.

## 1.6.0
- **A finger can be the stylus.** A new button on the right pill (a hand with one finger out, between the mode button and full screen) lets one finger on the
  page draw as the stylus does, with whatever tool is in use and in both modes, so that every tool works with no stylus (a phone, a touch laptop). It is
  remembered. A second finger calls the stroke off and the two pinch and pan (a stroke is begun only once the finger has moved a few pixels, so a pinch
  draws nothing), the toolbars, bar and cards are pressed by a finger as before, and a palm, or a hand beside a stylus that is near, is still ignored. With the
  button up a finger only pans and zooms, as it did.
- **New icons for the laser and for settings.** The laser is a pointer with its beam and the dot it makes (it was a sun); settings are a cog (they were
  three sliders). The cog is the bar's settings button and the round button of the look's pill when it is folded.
- **Only the flagged pages.** The overview's header has a flag button (left of the others): pressed, the grid shows only the pages that carry a flag, with the
  title "Flagged pages (3)"; pressed again, every page. A flagged page pressed is gone to; its menu (copy, flag, delete) works as in the full grid. The button
  is dim while no page has a flag; edit mode shows every page (to choose among and to move), and pages are not carried about while only some are shown. The
  header's six buttons are a little smaller in a narrow window. (Measured for the overview's CPU while the wheel scrolls it: building what is drawn costs
  about 0.15 ms a frame; the rest is the GPU draw, about a call for each picture and number, and 0% while still. See `ROADMAP.md` P2.)
- **Browsing through the document like the web.** The first link you follow inside a document (or contents entry, page pressed in the overview, or search
  result pressed) opens a small pill under the document's pill with back and forward buttons; they take you back to where you were, as you left it, and on
  again. Alt+Left and Alt+Right do the same (also on an Android keyboard), and so do a mouse's side buttons. A new jump after going back forgets what was
  ahead, scrolling by hand is not a jump, and editing the pages forgets the places. A page you jump to now has its first line below the toolbars (on a phone
  they covered the page's heading).
- **A card for links out to the web (an extension).** A link in a PDF that goes to the web is no longer handed to the browser unseen: a card says where it goes
  (the site's name first and plainly, then the whole address), warns of an address with a name in front of the site's own or a site written in letters that
  can stand for others, and has Open (in the system's browser), Copy and Close buttons. It is the new `weblink` extension, the first with no button on the
  toolbar (`LinkHandler` in `inkpdf-api`): the card is where a web view would go once there is an engine for one on every platform.
- **UI audit.** A test opens every card and pill at once in ten window sizes (a small phone to 4K, both ways round) and a storm of 32,000 random events (resizes,
  the keyboard, links, notes, searches, errors) checks that nothing floats outside the window or over another thing. It found, and this fixes: the round button
  under the theme button landing on the toolbar when its pill is folded; the cards at the right running over the back and forward pill and off the bottom of
  short windows (their rows now fit the window, a card that cannot fit goes up to be whole, and there is one card at the right at a time, the one opened last);
  and a window of any size (it cannot be made smaller than a small phone upright now).
- **Sticky notes, with replies.** A new tool (the note, after the laser): a click of the mouse or a tap of the stylus on the page puts a note there and
  opens its thread, a click on a note opens it, a drag moves it. The thread is a card at the right: who said what and when, a field to reply in (Enter or
  the arrow sends; Esc puts it away), a button to mark it done (or open again) and one to delete the note; your name is written on what you send
  (it starts as the system's user name and is changed in the card). A note is saved as a PDF text annotation with the replies to it, so any reader shows
  the thread, "done" is the review status Completed, and a note or reply made by another program is read the same way; notes of other programs that
  are not ours stay as the file has them. Undo, the lasso, the eraser, copy and paste, the page edits and search all work on notes. It needs only the left
  mouse button or the stylus tip.
- **Linux: the pen on Wayland.** Pressure, the eraser end, the side button, hover and the points between frames are read from the compositor's tablet
  protocol on the connection the window already has, so a pen on a Wayland tablet is a pen and not a mouse (X11, and a compositor with no tablet support,
  still read it as the mouse). Tried here only against a compositor of our own that plays a scripted pen (and WSLg, which has no tablet protocol): not on a real tablet.
- **Linux: the system clipboard,** an error card and the desktop's animation setting. Text copied in the program can be pasted in others and the other way
  (Wayland's data-control or X11; where neither is reachable the program keeps its own as before); a file that could not be opened or saved is said in a card
  over the page, on Linux and on Android, instead of on the terminal; and with "animations" off in the desktop's settings (GNOME's `enable-animations`) the
  program moves no more than it must (`INKPDF_MOTION=on` or `off` still decides for a run).
- **Android: the soft keyboard no longer hides what you type.** The keyboard covers the lowest part of the window without resizing it, so a text box
  (or form field) low on the page was typed under it. The window now tells the program how much of it the keyboard covers; the page glides up just far
  enough to bring what is typed above it (even at the end of the document), follows it as it grows, and goes back when the keyboard does, unless you
  scrolled. The window is also never drawn under a camera cut-out any more, with the bars hidden or not.
- **Extensions can make items (E3).** `inkpdf-api` has `ItemKind` (draw, write and read a PDF note, search text, a card) and the tool's `Context` can add,
  find, open, move and remove items; the program keeps the item as a stroke with a payload, so every edit follows it. The PDF seam gains the note
  annotation, its replies and a review status (`check_notes` in the conformance suite). The sticky notes are the first kind.
- **Fill in forms.** A PDF's form fields are lit faintly on the page and can be filled: a text field takes typed text (Enter or Esc ends it; a
  field of several lines takes new lines), a check box turns on and off, a radio button is chosen, and a choice goes to its next option. A finger
  does it with every tool; the stylus's tip and the left mouse button with the Select tool (the click that follows a link). What is put in is saved
  with the file (as real form values, with their appearances, so any reader shows them) and counts as a change. Read-only fields, push buttons,
  signatures and passwords are left alone; the form's JavaScript is not run. New: `pdfcraft-forms` (same pinned revision), `form.rs`, `inkpdf-gen --form`.
- **Flag a page.** The menu of a page in the overview has a fifth round button, a bookmark. A flagged page wears a bookmark in the
  overview; on save the file gets an outline entry "Flag: page N" for it, so any reader shows a bookmark. The program keeps its flags apart from the
  contents it shows, flags follow their pages through every page edit, and a flag counts as a change to save.
- **The look's card is a round button.** Pressing the theme button still goes round the system's look, light and dark, and a single round button (a
  moon on a page) hangs from it for whether the pages go dark too; the card with its words and its switch is gone.
- **One way to edit text.** The text box, the search field and the fields of a form share one text field (`field.rs`): the caret and Shift
  selection, arrows, Home and End, select all, copy, cut and paste work the same in all of them (a form field could only be typed into at its end).
- **Less code, the same behaviour.** The icons are listed once (a macro), the overview's round buttons and labels are drawn by one helper, the
  interface's and the overlays' line shapes by one `LineSink`, the settings file's keys by two small setters, and the editor's tests share their tap and
  drag helpers. The pen path is untouched.
- **Structure.** The ink store is made of the strokes, the undo history and the queries; an open document keeps no staged state (what a file holds comes
  out of opening it once, what a save adds goes in with the save) and its renderer is a part of its own; the window's cards share one press path; the
  workspace groups the styles, the look and the clipboards; opening and saving fail with a `DocError`.
- **The same real-input checks on every platform.** The 84 checks that drive the window (stylus, finger, mouse, keyboard, sticky notes, pages, reader, forms)
  are written once (`tools/e2e/scenarios.py`) and run on Windows, on Linux (a virtual display) and on the Android phone and tablet emulators, each
  with the devices the platform has: Windows' synthetic stylus and finger, X's mouse and keyboard with the stylus and finger handed to the new
  `--inject FILE` option, Android's input dispatcher through a small driver app (a pen and a pinch). A check a platform cannot do (the posture
  change) is reported as skipped. The program says where it is (page, pages, unsaved, mode) in its `--layout` file for the platforms with no window title.
  Found by it: a hardware keyboard's Page Up and Page Down did nothing on Android.
- **Two missed redraws fixed,** found by a new storm that checks every event that changes what is drawn asks for a frame: a finger's press on a toolbar
  button was not shown until something else drew, and nor was its release.
## 1.4.0

- **Input behaves like a reader (milestone 1.4.0, `docs/INPUT.md`).**
  - **The Select tool is a reader:** a drag on empty page pans with the mouse and the stylus (a tap only lets go of the selection), and the
    middle button of a mouse pans (in the drawing tools it is still the barrel button, a loop).
  - **Zoom and scroll keys at a desk:** Ctrl and the wheel zoom about the pointer; Ctrl+`+` and Ctrl+`-` zoom, Ctrl+0 fits the width, Ctrl+1
    shows the page at its size; Page Up and Page Down (and the space bar) scroll a screen, Home and End go to the top and the bottom, and the arrows scroll when
    nothing is selected for them to nudge. A mouse-only person could not zoom at all before. All of them are recorded and replayed.
  - **The cursor says what a press will do** (Windows and Linux; Android's pointer icons for a mouse or a hovering stylus): a hand over a link
    in the reader, an I-beam over text, an open hand over empty page and a closed one while it is dragged, a crosshair, a move cursor over what the
    lasso can take, a resize cursor on a selection's handle. It is one table, `input::decide`, that the cursor reads (and a test holds to
    the rules: a finger never draws, the mouse of a tablet only scrolls, a drawing tool never loses a press to a link or to text). It is asked after
    the mouse moves, never on the pen's path (a source test says so).
  - **One definition of a click** (`input/click.rs`): a finger's tap, a link followed by a click, a word or a line selected by two or three
    presses and the new double tap all read their slop and times from it, in logical pixels.
  - **A double tap fits the page to the width,** and a second double tap goes back to the zoom and place it came from. A single tap on a link
    still follows it at once, and a single tap on empty page still only lets go of the selection.
- **In a narrow window the pills fold into round buttons.** Where the document's, the tools' and the look's pills no longer fit side by side
  (a phone upright, a small window) the document's folds into a round button (a page), then the look's (sliders), then, in a very small
  window, the tools' (the tool in use). Pressing one unfolds its pill over the others, growing out of the button; the pill folds again when a
  button in it is pressed (not undo, redo and the look's), when a press is elsewhere, or on Esc. Buttons are 30 points or more before anything
  folds (they shrank to 17 before, and overlapped on a phone, so that a press could reach a hidden button). Every device does it with a press.
  The pages overview's title is dropped where it would run into the header's buttons.
- **Every icon is drawn in TeX**, the ones that were written by hand (the look, the search, the folder, the text marks, the dark page) and the
  ones that were drawn by code (the four pages, the tick, the arrows) too; `icons/build.ps1` makes all of them.
- **Text on buttons is now a picture.** The search card's "Aa" and "W" (match the case, whole words), the overview's "Select all / none" and
  the four words of a page's menu (now a row of round buttons), and the look's card ("Pages follow the theme", now a page half in shade and
  its switch).
- **A better icon for the system's look:** a sun and a moon, where it was a half-hatched circle.
- **Pulling up past the last page is smooth.** The document follows the finger off its end, more and more stiffly, with the plus in the room
  that opens (its ring fills as the pull nears the length that adds a page); let go early and it comes back down softly; long enough, and the
  page is added and the view glides on to it, slower than a scroll, while the document settles. The pull ends there: the finger can keep
  moving without stopping the glide.
- **The Android icon is the desktop one:** the same pen, on white; the adaptive icon is the pen alone, which Android 13 can also draw in one colour
  (the themed icon). `make-icon.ps1` makes the Windows and Android icons from one drawing (`pen-art.tex`); the old blue page and its
  Node script are gone.
- **The real-input checks press buttons by name.** `--layout FILE` makes the app keep a file saying where every toolbar button, bar
  action, overview page and contents entry is (`app/layout.rs`); `tools/e2e` reads it instead of remembering positions in a maximised
  4K window, which had gone stale (26 of 71 checks failed against 1.3.7 because the buttons had moved, not because anything was
  broken). The four scripts are now two, `devices.ps1` and `pages.ps1`: 50 checks, about a minute, all passing. They start the app in
  the light look so the colours they count do not depend on Windows' theme.

## 1.3.7

- **The system's own file dialogs on Windows and Linux** (Save As, and a new Open button, the folder at the left of the toolbar, as on Android),
  over the program's window, opening in the folder the document is in or in Documents. **The in-app file browser is gone**, with the code that
  only it used (folder listing, the "places", the file-name rules, the Save As card's colours). The dialogs run on a thread of their own, so the
  window goes on drawing while one is up. Linux needs the desktop portal for them (standard on GNOME, KDE and others). One thing to watch: the
  in-app card was built because a system window raised by a pen tap could stay hidden behind the program; the dialog is now given the window as
  its parent, which should keep it in front, but that has not been tried on a pen-only machine.

## 1.3.6

- **Android starts on a blank page**, like the other platforms, not on a welcome PDF (the PDF is no longer in the app). A document another app
  hands over ("Open with") or the Open button still opens as before.

## 1.3.5

- **The lasso and the eraser are tools** of the toolbar (after the text tool), not three buttons of the bar under the tools. The lasso's tip loops
  or taps to pick up items of any kind (ink, shapes, text boxes), drags what is picked up, and its bar has duplicate, copy, paste, delete, the
  colours and the settings for what it holds; the eraser's tip takes away whatever it passes over, of any kind. The stylus's eraser end and
  barrel button, and the mouse's right and middle buttons, still erase and loop with the pen, shape and text tools, each on its own kind.
  Up to ten tools fit the toolbar (it was eight); under about 550 points wide the pills no longer fit side by side.

## 1.3.4

- **Android: the full-screen button hides and shows the system bars** (status and navigation). It did nothing there, as Android's window cannot be
  made full screen; the button now asks the activity to hide the bars (a swipe from the edge shows them for a moment and they go away again,
  and the window draws under a camera cut-out), and its icon shows which it is. The program starts with the bars shown. On the desktop the button is as it was.

## 1.3.3

- The **Open button** (where the system has a file picker: Android) sits just left of Save, with the buttons that act on the document, not out among the page and search buttons.
- **Links work with every pointer.** A finger's tap was judged in screen pixels (10 px, 300 ms), so on a dense tablet screen the natural wobble of a
  fingertip made most taps "a swipe" and the link was never followed. The slop is now 12 logical pixels (scaled by the screen's scale), the
  time 350 ms, and a fling needs a speed in logical pixels too.
- With the **Select tool** (the one the program starts on, the one to read with) a click of the left mouse button or a tap of the stylus's tip on a
  link follows it, a drag from a link is a selection as before. The tools that draw still draw (only a finger's tap follows a link there).

## 1.3.2

- **Android follows the system's light and dark look** (set "system" with the theme button): the program reads the activity's night mode, and
  looks again each time its loop wakes, so switching the system theme while the app is open is followed.
- Android links: a link that cannot be opened is now said on the screen (a message) and in the log (`adb logcat -s inkpdf`), and the request
  carries the browsable category and a new task, as a browser expects. (If links still do nothing, the message says why.)

## 1.3.1

- The **Select tool** is the first button of the tools and the one the program starts on (a settings file that names another tool keeps it).
- Whether the **pages follow the theme** is a card that drops down from the theme button (which still goes system, light, dark), with one switch;
  it goes away on a press elsewhere, Esc or another button. It replaces the separate button. Works with the left mouse button and keyboard, the
  stylus tip and a finger.
- The Android app has an **icon**: an adaptive one (a white page with a blue ink stroke on the accent blue, which the launcher masks as a circle or a rounded square)
  and a plain one for launchers before Android 8. The pictures are `tools/android/icon/*.svg`; `tools/android/make-icons.js` makes the PNGs, and
  `build-apk.sh` compiles them into the APK.

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
- On Android the stylus's first button (the S Pen's one button) erases; a second button is the barrel button (lasso), and a
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
- The ends of a tap-drawn spline are no longer pointed: an open end leaves with no curvature, as in a natural
  spline, instead of running straight along the chord to the next point. Closed splines and the inner
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
