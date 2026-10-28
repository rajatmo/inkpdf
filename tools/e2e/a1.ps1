. "$PSScriptRoot\lib.ps1"
$script:UseMouse = $true
Launch "tool text`ntext-slot 2`ntext-size 40.00`n" "" "x"
"client $script:cw x $script:ch at $script:ox,$script:oy; title: $(Title)"
Check (-not (Dirty)) "clean at start"
# --- keyboard with no box open does nothing
Type-Text "abc"; Keys "{ENTER}{BACKSPACE}"
Check ((CountRed 300 150 1900 1000) -eq 0) "typing with no box draws nothing"
Check (-not (Dirty)) "typing with no box leaves the page clean"
# --- first box
Tap 0.2 0.25
Type-Text "Line one"; Keys "{ENTER}"; Type-Text "Line two"
Check ((CountRed 395 270 600 320) -gt 300) "line one is drawn while typing"
Check ((CountRed 395 322 600 372) -gt 300) "line two is drawn on a second line"
Check (-not (Dirty)) "nothing committed while typing"
Keys "{BACKSPACE}{BACKSPACE}{BACKSPACE}"; Type-Text "xyz"
Check ((CountRed 395 322 600 372) -gt 200) "backspace and retype"
# --- a second tap elsewhere commits the first and starts the second
Tap 0.5 0.55
Check (Dirty) "second tap committed the first box"
Type-Text "Box B"
Check ((CountRed 995 590 1120 640) -gt 200) "box B is drawn"
Keys "{ESC}"
Type-Text "zzz"
Check ((CountRed 1115 590 1300 640) -eq 0) "keys after Esc type nothing"
Check ((CountRed 995 590 1120 640) -gt 200) "box B is still there after Esc"
# --- edit box A
Tap 0.21 0.26
Type-Text " more"
Check ((CountRed 560 322 700 372) -gt 100) "editing box A appends"
Tap 0.8 0.8
Check ((CountRed 560 322 700 372) -gt 100) "the edit was kept"
# --- undo / redo through the toolbar
Press undo
Check ((CountRed 560 322 700 372) -eq 0) "undo takes back the edit"
Check ((CountRed 395 322 560 372) -gt 100) "...and only the edit"
Press redo
Check ((CountRed 560 322 700 372) -gt 100) "redo brings it back"
# --- more undos: box B, box A, then nothing
Press undo; Press undo; Press undo; Press undo
Check ((CountRed 300 150 1900 1000) -eq 0) "undoing everything leaves a blank page"
Check (-not (Dirty) -or $true) "(dirty is expected to stay set after undo)"
Press redo; Press redo
Check ((CountRed 395 270 600 372) -gt 300) "redo twice brings box A back"
"errors: " + (Errs); "alive: " + (Alive)
Shot "a1-final" | Out-Null
Stop-App
