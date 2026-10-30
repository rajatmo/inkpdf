# The devices, for real: that the stylus, a finger, the mouse and the keyboard reach the app and do their work in both modes,
# that the tools draw on screen, and that a posture change from Windows is heard. Which device may do what in every tool and
# mode, and every edge, is for the unit and storm tests; this proves the real window and the real input reach them.
. "$PSScriptRoot\lib.ps1"

# ---- 1. Desktop mode: each device, the tools, undo.
Launch "tool pen`nslot 0`nshape-slot 3`ntext-slot 2`ntext-size 40.00`n" "" @("--mode", "desktop")
B stroke 0.3 0.5 0.7 0.5 0.03 2 40
$ink = Black
Check (($ink -gt 500) -and (Dirty)) "the stylus draws ($ink px), and the document is marked changed"
Press undo
Check ((Black) -eq 0) "undo, pressed with the mouse, takes it away"
B drag 0.5 0.4 0.5 0.6 15; B pinch 0.5 0.5 0.05 0.2 15; B pinch 0.5 0.5 0.2 0.05 15
Check ((Black) -eq 0) "a finger pans and zooms and draws nothing"
Press redo
Check ((Black) -gt 500) "redo brings it back"
$one = Black
Mouse-Drag 0.3 0.7 0.7 0.7
Check ((Black) -gt $one * 1.5) "the mouse draws too, in the desktop mode"
Keys "^z"
Check ([math]::Abs((Black) - $one) -lt $one * 0.05) "Ctrl+Z, from the keyboard, takes that back"
Press geometry
B line 0.3 0.25 0.6 0.35 20
Check ((Accent) -gt 50) "the shape tool: a stylus drag makes a shape and its anchors show"
Press text
Click 0.5 0.85; Type-Text "Hi"; Keys "{ESC}"
Check ((Red) -gt 150) "the text tool: a mouse click and the keyboard make text"
Press lasso
Click 0.5 0.5
Check ((Accent) -gt 100) "the lasso: a click on the stroke selects it"
Keys "{DEL}"
Check ((Black) -eq 0) "Delete takes the selection away"
Keys "^z"
Press eraser
Mouse-Drag 0.5 0.4 0.5 0.6
Check ((Black) -eq 0) "the eraser: the left button erases what it passes over"
Keys "^z"
Press pen
$red = Red
Bar swatch2
Mouse-Drag 0.3 0.65 0.7 0.65
Check ((Red) -gt $red + 500) "a colour pressed in the bar is the colour of the next stroke"
Keys "^z"; Bar swatch0

# ---- 2. Tablet mode: the mode button, the stylus tip alone (no barrel button, no eraser end), a box's text to the clipboard.
$mode = Spot buttons mode
Check ((Count-Near $script:accent $mode) -lt 5) "desktop mode: the mode button is plain"
Press mode
Check ((Count-Near $script:accent $mode) -gt 30) "one press: tablet mode, and the button shows it"
$ink = Black
Mouse-Drag 0.3 0.6 0.7 0.6
Check ((Black) -eq $ink) "the tablet's mouse draws nothing"
Pen-Press lasso
B tap 0.5 0.5
Check ((Accent) -gt 100) "with the tip alone: the lasso's tap selects the stroke"
Pen-Bar delete
Check ((Black) -eq 0) "and the bar's delete, tapped, takes it away"
Pen-Press undo
Check ((Black) -gt 500) "undo, tapped, brings it back"
Pen-Press text
B tap 0.5 0.75; Type-Text "ab"
Keys "^a"; Keys "^c"
Check (((Get-Clipboard -Raw) -replace "\s+$", "") -eq "ab") "Ctrl+A, Ctrl+C in a box: its words are on the Windows clipboard"
Keys "{ESC}"
Tell-Setting "ConvertibleSlateMode"
Check ((Count-Near $script:accent $mode) -gt 30) "a mode given on the command line is not changed by the posture"
Press mode
Check ((Count-Near $script:accent $mode) -lt 5) "another press: back at the desk"
Check (Healthy) "no crash, nothing printed to stderr"

# ---- 3. No mode given: the app follows the machine (run this on a desktop PC: its mouse must draw).
Launch "tool pen`nslot 0`n" ""
Mouse-Drag 0.3 0.5 0.7 0.5
Check ((Black) -gt 500) "no mode given: this desktop PC starts in the desktop mode, and its mouse draws"
Press undo
Press mode
Mouse-Drag 0.3 0.5 0.7 0.5
Check ((Black) -eq 0) "the button made it a tablet"
Tell-Setting "ConvertibleSlateMode"
Mouse-Drag 0.3 0.5 0.7 0.5
Check ((Black) -gt 500) "a posture change: the app looks at the machine again, and it is a desktop"
Check (Healthy) "no crash, nothing printed to stderr"
Stop-App
