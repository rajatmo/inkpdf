InkPDF - a fast, pen-first PDF annotator for Windows

InkPDF opens maximized on a blank page the size of your display: just start writing. To work on a
PDF instead, drag it onto the window or use "Open with". Press the disk button to save: a new
page asks where to put the file, and a PDF you opened is updated in place. Your ink and shapes are
saved inside the PDF as standard Ink annotations, and your text as standard Free Text annotations,
so any PDF reader shows them; nothing else is kept.

InkPDF has two modes, and picks one itself from how the machine is being used: DESKTOP (at a desk,
with a keyboard and mouse) or TABLET (a convertible turned into a slate, or a tablet). They differ in
what the mouse and the keyboard do; the stylus and the finger do the same in both, except that the
stylus can also place text in the tablet mode. On a desktop PC the mode starts as desktop. The button at the
top right, left of full screen, shows the mode and switches it whenever you like; when the machine is
turned into a tablet or back, InkPDF looks at it again and follows. To start in one: inkpdf --mode desktop
or inkpdf --mode tablet (the default is --mode auto).

Each kind of input does one thing, and what the stylus does depends on the tool:

  STYLUS    does everything to the page (with the text tool, only in the tablet mode). Its tip works
            with the tool you pick; its eraser end erases whole strokes; its barrel button draws a
            lasso.
              Pen tool      draws ink, erases, lassos
              Shape tool    selects shapes (a tap on one), draws, erases, lassos
                            Line and curve: tap on empty page to start a polyline or a spline, tap
                              again for every node, double-tap to end it; a double-tap near the first
                              node closes it into a polygon (or a closed curve). (A drag from one
                              point to another still makes a two-node line or curve at once.)
                            Rectangle, ellipse, axes (2D, 3D): drag them out.
                            A selected shape shows the anchors that suit it:
                              lines      a node for every corner; double-tap the line to add one,
                                         double-tap an end node to close it into a polygon (or a node
                                         of a polygon to open it), drag an end onto the other end to
                                         close it; the barrel button on a node takes it away, and
                                         tapped on empty page it adds a node at the end; nodes snap
                                         to other shapes; segments always stay straight
                              curves     anchors with tangent handles; double-tap an anchor to switch
                                         it between smooth and sharp, double-tap the curve to add one
                              rectangles the two ends of the diagonal
                              ellipses   the four corners of the bounding rectangle
                              axes       the origin (the lines follow it) and the end of each axis,
                                         which slides along its axis, so they stay perpendicular
              Pointer tool  a red dot and a fading trail, for presenting; draws nothing
              Text tool     desktop mode: nothing, the stylus does not touch the page (it still presses
                            buttons). Tablet mode: it places, selects and moves text, as the mouse does
                            at a desk (type with the touch keyboard or a keyboard).
  TOUCH     pans and zooms; a tap on a link follows it (web links open in your browser);
            swipe up past the last page to add a blank page the size of the last one
  MOUSE     the wheel scrolls, and the mouse presses the toolbar's buttons, in both modes.
            Desktop mode: the mouse also does what the stylus does with the pen, shape and pointer
              tools: the left button is the tip (draws, selects, moves), the right button is the
              eraser end, the middle button is the barrel button (lasso). With the text tool it places,
              selects and moves text: click empty page to start a text box, click a box to edit it,
              drag a box to move it.
            Tablet mode: it only scrolls.
  KEYBOARD  types into the text box being edited (Enter starts a line, Backspace and Delete take a
            character away, Esc finishes; lines wrap at the page's right edge). The caret goes where you
            press in the box; the arrows, Home and End move it, with Shift they select (so does a drag);
            Ctrl+A, C, X and V work on the text with the Windows clipboard. Undo takes back what you have
            typed first. In the desktop mode Ctrl+Z undoes, Ctrl+Y or Ctrl+Shift+Z redoes, Ctrl+S saves,
            and the keyboard works on the selection (see below).

SELECTING AND EDITING. Every tool works on the items it made (the pen on handwriting, the shape tool on
shapes, the text tool on text boxes), with whatever the mode gives you: at a desk the left mouse button and
the keyboard, as a tablet the tip of the stylus. The bar under the tools has the buttons:
  draw / select / erase   what the plain tip (or the left mouse button) does. With select chosen, tap an
                          item, or draw a loop round some, to select them; drag them to move them. With
                          erase chosen, the tip erases what it passes over.
  duplicate, copy, paste, delete
                          work on what is selected (paste puts the copy in the middle of what you see).
  three buttons for a point of a line or curve being edited: add a point, take the one you touched last
                          away, smooth it or sharpen it.
  five colours            one tap gives the tool, and what is selected, that colour; a tap on the colour
                          in use opens its editor. (On a narrow window they are one dot that opens the panel.)
  settings                unfolds the tool's panel from the bar: thickness (a text box's size), smoothing,
                          the shape. What you choose changes what is selected too.
At a desk: Ctrl+A selects all of the tool's items on the page, Ctrl+C copies, Ctrl+X cuts, Ctrl+V pastes,
Ctrl+D duplicates, Delete deletes, Esc lets go, the arrows nudge (with Shift, further). With the shape tool,
Backspace takes away the point you touched last. The barrel button, the eraser end and the mouse's right
and middle buttons are shortcuts for select and erase; nothing needs them.
The buttons: X closes the document and starts a blank page (it asks first if there is unsaved
work); undo, redo, save; the bin deletes the page you are looking at, at once, with no question
asked (the last page there is stays; undo brings it back, with its ink, until you save); + adds a
page after the one you are looking at; the four small pages open the pages overview (below); then the pen, shape, text and pointer tools
(a tool's button only chooses it; its settings are in the bar under the tools); a monitor or a tablet, which shows the
mode in use (the tablet in blue) and switches to the other when pressed; full screen. (Under the tools:
the bar described above.)

PAGES OVERVIEW. The four-pages button opens a card over the document: a picture of every page (with its ink), its
number, and on the left the PDF's own table of contents when it has one. The page you are looking at is ringed and
its section lit. Tap a page to go to it; tap an entry of the contents to go there (its arrow folds it); scroll with the
wheel, a finger or by dragging empty space. Header: + adds a page after the one you were looking at, the check mark is
edit mode, undo and redo (the same as the document's), X closes (so do Esc and a tap on the dimmed document).
In edit mode a tap chooses a page (a tick shows) or lets go of it, and a bar under the pages offers: Select all / none,
duplicate, move earlier, move later and delete (all of it for the chosen pages). Dragging a page with the stylus or the
mouse moves it (all the chosen ones together if it is one of them): the blue bar shows where it will land, and holding it
near the top or bottom scrolls. A finger scrolls; use the arrows of the bar to move pages with it. The number under a page
opens its menu: add a page before, add a page after, duplicate, delete. Every one of these is a step of the history. At a
desk: Ctrl+A chooses every page, Ctrl+D duplicates the chosen ones, Delete deletes them, Ctrl+Z and Ctrl+Y undo and redo,
and Esc steps out one level at a time (a drag, the chosen pages, edit mode, the overview).
Your tools and their settings are kept in %APPDATA%\InkPDF\settings.txt.

When InkPDF is your default PDF app (Settings > Apps > Default apps), Explorer shows the first
page of each PDF as its icon.

Command line:  inkpdf [file.pdf] [--fullscreen] [--smoothing 0..1] [--gpu integrated|discrete|auto]
               [--mode desktop|tablet|auto]
Licence: MIT or Apache-2.0 (see the LICENSE files).