# The mode button and the posture, in two app starts: the button toggles desktop and tablet and shows which is which, the
# devices follow, and Windows telling the app the posture changed makes it look again (unless the mode was given on
# the command line). The second start follows the machine, so run it on a desktop PC (its mouse must draw).
. "$PSScriptRoot\lib.ps1"
$button = @(1920, 11, 1960, 50)     # where the mode button is, in the window's 2000 x 1070 units

# ---- 1. The button, with the mode given on the command line.
Launch "tool pen`nslot 0`ntext-slot 2`ntext-size 40.00`nshape-slot 3`n" "" @("--mode", "desktop")
Check ((Accent @button) -lt 5) "desktop mode: the button is plain"
Mouse-Drag 0.3 0.5 0.7 0.5
Check ((Black) -gt 500) "and the mouse draws"
# The bar under the tools, the left button and the keyboard alone: choose select, tap the stroke, delete it, bring it back.
Bar select
Click 0.5 0.5
Check ((Accent 300 150 1900 1000) -gt 100) "select chosen: a tap on the stroke selects it (its frame shows)"
Keys "{DEL}"
Check ((Black) -eq 0) "Delete takes the selection away"
Keys "^z"
Check ((Black) -gt 500) "and Ctrl+Z brings it back"
Bar erase
Mouse-Drag 0.5 0.4 0.5 0.6
Check ((Black) -eq 0) "erase chosen: the left button erases what it passes over"
Keys "^z"
Bar draw
Check ((Black) -gt 500) "and draw chosen, undone: back to ink"
Keys "^z"
Check ((Black) -eq 0) "and Ctrl+Z undoes"
Bar red
Mouse-Drag 0.3 0.6 0.7 0.6
Check ((Red) -gt 500) "a colour tapped in the bar is the colour of the next stroke, in one tap"
Keys "^z"
Bar black
Press mode
Check ((Accent @button) -gt 30) "one press: tablet mode, and the button shows it in the accent colour"
Mouse-Drag 0.3 0.5 0.7 0.5
Check ((Black) -eq 0) "the tablet's mouse draws nothing"
B stroke 0.3 0.5 0.7 0.5 0.03 2 40
$ink = Black
Check ($ink -gt 500) "the stylus draws"
Keys "^z"
Check ((Black) -eq $ink) "Ctrl+Z does nothing"
# As a tablet, with the tip alone (no barrel button, no eraser end): select the stroke, delete it from the bar, bring it back.
Pen-Bar select
B tap 0.5 0.5
Check ((Accent 300 150 1900 1000) -gt 100) "as a tablet, select chosen: a tap of the stylus tip selects the stroke"
Pen-Bar delete
Check ((Black) -eq 0) "and the bar's delete button takes it away, with the tip alone"
Press undo
Check ((Black) -gt 500) "undo brings it back"
Pen-Bar draw
Press undo
Use-Tool text
B tap 0.5 0.5; Type-Text "ab"
Keys "^a"; Keys "^c"
Check (((Get-Clipboard -Raw) -replace "\s+$", "") -eq "ab") "Ctrl+A, Ctrl+C in a box: its words are on the Windows clipboard"
B tap 0.8 0.85
Check ((Red) -gt 100) "a stylus tap places text, as the mouse does at a desk"
Tell-Setting "ConvertibleSlateMode"
Check ((Accent @button) -gt 30) "a mode given on the command line is not changed by the posture"
Press mode
Check ((Accent @button) -lt 5) "another press: back at the desk, the button plain"
Use-Tool pen
Mouse-Drag 0.3 0.7 0.7 0.7
Check ((Black) -gt 500) "and the mouse draws again"

# A tool offers buttons of its own: a line that is selected has points, and the bar can add one (the points show as accent squares).
Use-Tool shape
Mouse-Drag 0.3 0.4 0.6 0.6
$two = Accent 300 150 1900 1000
Check ($two -gt 100) "the shape tool makes a line with the mouse and it is selected (its two ends show)"
Bar add
$three = Accent 300 150 1900 1000
Check (($three - $two) -gt 60) "the bar offers the shape its own buttons: one press adds a point to the line"
Keys "^z"
Check (((Accent 300 150 1900 1000) - $three) -lt -40) "and Ctrl+Z takes the point away again (fewer points show than with three)"
Use-Tool pen

# ---- 2. The mode follows the machine.
Launch "tool pen`nslot 0`n" ""
Mouse-Drag 0.3 0.5 0.7 0.5
Check ((Black) -gt 500) "no mode given: this desktop PC starts in the desktop mode, and its mouse draws"
Press undo
Press mode
Mouse-Drag 0.3 0.5 0.7 0.5
Check ((Black) -eq 0) "the button made it a tablet"
Tell-Setting "ImmersiveColorSet"
Mouse-Drag 0.3 0.5 0.7 0.5
Check ((Black) -eq 0) "a setting that is not the posture changes nothing"
Tell-Setting "ConvertibleSlateMode"
Mouse-Drag 0.3 0.5 0.7 0.5
Check ((Black) -gt 500) "a posture change: the app looks at the machine again, and it is a desktop"
Check ((Alive) -and ((Errs).Count -eq 0)) "no crash, nothing printed to stderr"
Stop-App
