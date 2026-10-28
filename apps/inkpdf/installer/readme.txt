InkPDF - a fast, pen-first PDF annotator for Windows

InkPDF opens maximized on a blank page the size of your display: just start writing. To work on a
PDF instead, drag it onto the window or use "Open with". Press the disk button to save: a new
page asks where to put the file, and a PDF you opened is updated in place. Your ink and shapes are
saved inside the PDF as standard Ink annotations, and your text as standard Free Text annotations,
so any PDF reader shows them; nothing else is kept.

Each kind of input does one thing, and what the stylus does depends on the tool:

  STYLUS    does everything to the page, except with the text tool. Its tip works with the tool you
            pick; its eraser end erases whole strokes; its barrel button draws a lasso.
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
              Text tool     nothing: the stylus does not touch the page (it still presses buttons)
  TOUCH     pans and zooms; a tap on a link follows it (web links open in your browser);
            swipe up past the last page to add a blank page the size of the last one
  MOUSE     scrolls (it also presses the toolbar's buttons); with the text tool it also places,
            selects and moves text: click empty page to start a text box, click a box to edit it,
            drag a box to move it
  KEYBOARD  types into the text box being edited (Enter starts a line, Backspace takes a character
            away, Esc finishes; lines wrap at the page's right edge); it does nothing else. Undo
            takes back what you have typed first.
The buttons: X closes the document and starts a blank page (it asks first if there is unsaved
work); undo, redo, save; + adds a page after the one you are looking at; then the pen, shape, text and
pointer tools (tap a tool again to open its colours, thickness and options); full screen.
Your tools and their settings are kept in %APPDATA%\InkPDF\settings.txt.

When InkPDF is your default PDF app (Settings > Apps > Default apps), Explorer shows the first
page of each PDF as its icon.

Command line:  inkpdf [file.pdf] [--fullscreen] [--smoothing 0..1] [--gpu integrated|discrete|auto]
Licence: MIT or Apache-2.0 (see the LICENSE files).