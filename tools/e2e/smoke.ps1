# What only real input can show, in two app starts: that the stylus, a finger, the mouse and the keyboard reach the app
# and do their work, that the tools draw on screen, and that the pages and the saved file are right. The rules
# themselves (which device does what in each tool and mode, every edge) are the unit and stress tests'.
. "$PSScriptRoot\lib.ps1"

# ---- 1. A blank page: stylus, finger, mouse, keyboard, three tools, undo.
Launch "tool pen`nslot 0`nshape-slot 0`ntext-slot 2`ntext-size 40.00`n" "" @("--mode", "desktop")
B stroke 0.3 0.5 0.7 0.5 0.03 2 40
$ink = Black
Check (($ink -gt 500) -and (Dirty)) "the stylus draws ($ink px), and the document is marked changed"
Press undo
Check ((Black) -eq 0) "undo, a toolbar button pressed with the mouse, takes it away"
B drag 0.5 0.4 0.5 0.6 15; B pinch 0.5 0.5 0.05 0.2 15; B pinch 0.5 0.5 0.2 0.05 15
Check ((Black) -eq 0) "a finger pans and zooms and draws nothing"
Press redo
Check ((Black) -gt 500) "redo brings it back"
$before = Black
Mouse-Drag 0.3 0.7 0.7 0.7
Check ((Black) -gt $before * 1.5) "the mouse draws too, in the desktop mode"
Keys "^z"
Check ([math]::Abs((Black) - $before) -lt $before * 0.05) "Ctrl+Z, from the keyboard, takes that back"
Use-Tool shape
B line 0.3 0.25 0.6 0.35 20
Check ((Accent 300 150 1900 1000) -gt 50) "the shape tool: a stylus drag makes a shape and its anchors show"
Use-Tool text
Click 0.5 0.85; Type-Text "Hi"; Keys "{ESC}"
Check ((Red) -gt 150) "the text tool: a mouse click and the keyboard make text"
Check ((Alive) -and ((Errs).Count -eq 0)) "no crash, nothing printed to stderr"

# ---- 2. A three-page PDF: delete and add pages with every device, save, reopen.
Launch "tool pen`nslot 2`nwidth 6.00`n" "pages3" @("--mode", "desktop")
Check (((Pages)[1] -eq 3) -and (-not (Dirty))) "a three-page PDF opens ($(Title))"
B stroke 0.3 0.5 0.7 0.5 0.03 2 40
Check ((Red) -gt 500) "ink on page 1"
Press del
Check (((Pages)[1] -eq 2) -and ((Red) -eq 0)) "the bin deletes the page with one press, and its ink goes with it ($(Title))"
Press plus
Check ((Pages)[1] -eq 3) "+ adds a page ($(Title))"
Pen-Press del
Check ((Pages)[1] -eq 2) "the stylus presses the same button"
Start-Sleep -Milliseconds 700   # (a finger is ignored while the pen was just near)
Finger-Press del
Check ((Pages)[1] -eq 1) "and a finger"
Press del
Check ((Pages)[1] -eq 1) "the last page there is stays"
Press save
Check (-not (Dirty)) "save writes the file ($(Title))"
Reopen
Check ((Pages)[1] -eq 1) "reopened: the file has the one page that is left"
Stop-App

# ---- 3. A spline through eight points of a circle, left open: its ends follow the circle (they do not cut in along the chord).
Launch "tool geometry`nshape curve`nshape-slot 3`nshape-width 3.00`n" "" @("--mode", "desktop")
foreach ($k in 0..7) { $a = -[math]::PI / 2 + $k * [math]::PI / 4; Click (0.5 + 0.15 * [math]::Cos($a)) (0.58 + 0.28 * [math]::Sin($a)) }
Use-Tool pen     # leaves the shape tool: the spline is made
Check ((Count 38 160 84) -gt 600) "eight taps make a spline of green ink"
# The window is 2000 x 1070 units; the circle's centre is (1000, 620) and its radius 300. The chord from a tip's node to its neighbour
# passes 23 units inside the circle at its middle; a curve that leaves the node along the circle does not.
$cut = (Count 38 160 84 1099 357 1113 371) + (Count 38 160 84 737 507 751 521)
Check ($cut -eq 0) "the tips follow the circle: no ink on the chords ($cut px)"
Check ((Alive) -and ((Errs).Count -eq 0)) "no crash, nothing printed to stderr"
Stop-App
