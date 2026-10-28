. "$PSScriptRoot\lib.ps1"
function Mtap($x,$y) { Mouse-Click $x $y }
function Slow { Start-Sleep -Milliseconds 700 }
# ====== TEXT TOOL: keyboard types, mouse scrolls and selects text, touch pans/zooms, pen does nothing ======
Launch "tool text`ntext-slot 2`ntext-size 40.00`n" "" "x"
function Red { CountRed 300 150 1900 1000 }
# the pen does nothing with the text tool: tip, eraser end, barrel button
Tap 0.2 0.25; Type-Text "abc"
B flag eraser on tap 0.2 0.25
B flag barrel on tap 0.2 0.25
B line 0.2 0.25 0.4 0.4 8
Type-Text "def"
Check (((Red) -eq 0) -and -not (Dirty)) "the pen (tip, eraser end, barrel) starts no text and changes nothing"
# a finger does nothing either
B finger down 1 0.2 0.25; B finger up 1 0.2 0.25
Type-Text "ghi"
Check ((Red) -eq 0) "a finger tap starts no text"
# the mouse places a box; the keyboard types
Mtap 0.2 0.25
Type-Text "Mouse text"
$r1 = Red
Check ($r1 -gt 800) "a mouse click starts a text box and the keyboard types into it ($r1 px)"
Mtap 0.5 0.55
Check (Dirty) "a click elsewhere ends it (and begins the next box)"
Type-Text "second"
Keys "{ESC}"
Check ((CountRed 990 590 1400 650) -gt 400) "the second box is where it was clicked"
# a click on a box selects/edits it; the keyboard appends
Mtap 0.21 0.26
Type-Text "!"
$r2 = CountRed 395 270 1000 330
Mtap 0.8 0.9; Keys "{ESC}"
Check ($r2 -gt 800) "a mouse click on a box edits it"
# the pen cannot edit it
Tap 0.21 0.26
$before = Red
Type-Text "zzz"
Check ((Red) -eq $before) "the pen cannot select or edit a text box"
# a mouse drag moves the box
$b0 = CountRed 395 270 1000 330
Mouse-Drag 0.21 0.26 0.4 0.8
Check (((CountRed 395 270 1000 330) -lt 40) -and ((CountRed 790 840 1400 920) -gt 800)) "a mouse drag moved the box ($b0 px at the old place)"
# the wheel still scrolls, and the pen cannot move it
Wheel (-1); Wheel 1
Press undo
Check ((CountRed 395 270 1000 330) -gt 800) "undo put the moved box back"
"errors: " + (Errs); "alive: " + (Alive)
Stop-App

# ====== POLYLINE: tap starts, each tap adds a node, a double tap ends, near the first closes ======
Launch "tool geometry`nshape line`nshape-slot 0`n" "" "x"
Tap 0.2 0.5; Slow
Tap 0.4 0.4; Slow
Tap 0.6 0.6; Slow
Snap
Check ((Dirty) -eq $false) "nothing is stored while the stroke is being drawn"
Check (((Straight 400 535 800 428) -ge 0.95) -and ((Straight 800 428 1200 642) -ge 0.95)) "the stroke so far is drawn: straight segments through the tapped nodes"
Check (((Accent 385 520 415 550) -gt 6) -and ((Accent 785 413 815 443) -gt 6) -and ((Accent 1185 627 1215 657) -gt 6)) "a square at each node"
Dbl 0.8 0.5
Snap
Check ((Dirty)) "the double tap ended the stroke: it is part of the document now"
Check (((Straight 400 535 800 428) -ge 0.95) -and ((Straight 800 428 1200 642) -ge 0.95) -and ((Straight 1200 642 1600 535) -ge 0.95)) "an open polyline of four nodes, every segment straight ($(Straight 1200 642 1600 535))"
Check ((Accent 1585 520 1615 550) -gt 6) "its anchors show (it is selected)"
Press undo
Snap
Check ((Straight 400 535 800 428) -lt 0.3) "one undo takes the whole polyline away"
# a closed polygon: a double tap near the first node
Tap 0.2 0.3; Slow
Tap 0.5 0.3; Slow
Tap 0.35 0.5; Slow
Dbl 0.202 0.302
Snap
Check (((Straight 400 321 1000 321) -ge 0.95) -and ((Straight 1000 321 700 535) -ge 0.95) -and ((Straight 700 535 400 321) -ge 0.95)) "a double tap near the first node closed the triangle ($(Straight 700 535 400 321))"
Shot "t1-1-triangle" | Out-Null
# undo while drawing takes back the last node
UseTool pen; UseTool shape
Tap 0.2 0.8; Slow
Tap 0.4 0.8; Slow
Tap 0.4 0.9; Slow
Press undo
Snap
Check (((Straight 400 856 800 856) -ge 0.95) -and ((Straight 800 856 800 963) -lt 0.3)) "undo while drawing took back the last node"
# switching tool ends the stroke as it is
UseTool pen
Snap
Check ((Dirty) -and ((Straight 400 856 800 856) -ge 0.95)) "choosing another tool ended the stroke with its two nodes"

# ====== SPLINE: the same, with smooth curves through the nodes ======
Launch "tool geometry`nshape curve`nshape-slot 0`n" "" "x"
Tap 0.2 0.6; Slow
Tap 0.4 0.3; Slow
Tap 0.6 0.6; Slow
Dbl 0.8 0.3
Snap
Check (((Extent 385 610 415 670)[0] -ge 0) -and ((Extent 785 300 815 340)[0] -ge 0) -and ((Extent 1185 610 1215 670)[0] -ge 0)) "the spline passes through the tapped nodes"
Check ((Straight 400 642 800 321) -lt 0.85) "and is a curve, not straight segments ($(Straight 400 642 800 321))"
Check ((Accent 1585 300 1615 340) -gt 6) "the last node is where the double tap was"
Shot "t1-2-spline" | Out-Null
"errors: " + (Errs); "alive: " + (Alive)
Stop-App
