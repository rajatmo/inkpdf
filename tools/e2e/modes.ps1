# The mode button and the posture, in two app starts: the button toggles desktop and tablet and shows which is which, the
# devices follow, and Windows telling the app the posture changed makes it look again (unless the mode was given on
# the command line). The second start follows the machine, so run it on a desktop PC (its mouse must draw).
. "$PSScriptRoot\lib.ps1"
$button = @(1913, 13, 1953, 53)     # where the mode button is, in the window's 2000 x 1070 units

# ---- 1. The button, with the mode given on the command line.
Launch "tool pen`nslot 0`ntext-slot 2`ntext-size 40.00`n" "" @("--mode", "desktop")
Check ((Accent @button) -lt 5) "desktop mode: the button is plain"
Mouse-Drag 0.3 0.5 0.7 0.5
Check ((Black) -gt 500) "and the mouse draws"
Keys "^z"
Check ((Black) -eq 0) "and Ctrl+Z undoes"
Press mode
Check ((Accent @button) -gt 30) "one press: tablet mode, and the button shows it in the accent colour"
Mouse-Drag 0.3 0.5 0.7 0.5
Check ((Black) -eq 0) "the tablet's mouse draws nothing"
B stroke 0.3 0.5 0.7 0.5 0.03 2 40
$ink = Black
Check ($ink -gt 500) "the stylus draws"
Keys "^z"
Check ((Black) -eq $ink) "Ctrl+Z does nothing"
Press undo
Use-Tool text
B tap 0.5 0.5; Type-Text "ab"; B tap 0.8 0.85
Check ((Red) -gt 100) "a stylus tap places text, as the mouse does at a desk"
Tell-Setting "ConvertibleSlateMode"
Check ((Accent @button) -gt 30) "a mode given on the command line is not changed by the posture"
Press mode
Check ((Accent @button) -lt 5) "another press: back at the desk, the button plain"
Use-Tool pen
Mouse-Drag 0.3 0.7 0.7 0.7
Check ((Black) -gt 500) "and the mouse draws again"

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
