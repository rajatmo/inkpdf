. "$PSScriptRoot\lib.ps1"
# PEN, POINTER (laser), TOUCH, MOUSE, KEYBOARD, TOOLBAR, in the tablet mode (where the mouse only scrolls;
# r3 checks the desktop mode, where it draws).
$script:extraArgs = @("--mode", "tablet")
Launch "tool pen`n" "" "x"
function Ink { Count 28 30 36 0 100 2000 1070 }
Check (-not (Dirty)) "starts clean"
# --- pen: draws ink
Drag 0.2 0.3 0.5 0.3 20
$i1 = Ink
Check ($i1 -gt 400) "the pen draws ink ($i1 px)"
Check (Dirty) "and the document is marked changed"
# --- keyboard does nothing with the pen
Type-Text "typing"; Keys "{ENTER}{BACKSPACE}{ESC}"
Check ((Ink) -eq $i1) "the keyboard does nothing in the pen tool"
# --- mouse: scrolls only
Mouse-Click 0.6 0.6; Mouse-Drag 0.6 0.6 0.8 0.8
Check ((Ink) -eq $i1) "the mouse draws nothing"
Wheel (-2); Wheel 2; Wheel 10; Start-Sleep 1
# --- touch: pans and zooms, draws nothing
B finger down 1 0.7 0.6; B finger up 1 0.7 0.6
B drag 0.7 0.7 0.7 0.6 8
Wheel 30; Start-Sleep 1
Check (((Ink) -eq $i1) -or ([math]::Abs((Ink) - $i1) -lt $i1 * 0.02)) "a finger draws nothing (the page was panned, ink unchanged: $i1 -> $(Ink))"
# --- barrel-button lasso selects, the tip drags the selection
Lasso 0.15 0.25 0.55 0.35
Check ((Accent 100 150 1200 500) -gt 40) "the barrel-button lasso selected the stroke (frame shows)"
Drag 0.35 0.3 0.35 0.5 10
Snap
Check (((Extent 300 520 1100 550)[0] -ge 0) -and ((Extent 300 300 1100 340)[0] -lt 0)) "dragging the selection with the tip moved the ink down"
Press undo
Snap
Check ((Extent 300 300 1100 340)[0] -ge 0) "undo put it back"
Press redo
Press undo
Tap 0.9 0.95
# --- the colour and thickness panel
Press pen
Tap 0.4465 0.2336
Tap 0.95 0.95
Drag 0.2 0.6 0.5 0.6 20
$thin = CountBlue 300 600 1100 700
Check ($thin -gt 300) "a colour chosen in the panel is what the pen writes in ($thin blue px)"
Press pen
Tap 0.523 0.1636
Tap 0.95 0.95
Drag 0.2 0.75 0.5 0.75 20
$thick = CountBlue 300 750 1100 900
Check ($thick -gt 3 * $thin) "a thicker pen draws a much thicker line ($thin -> $thick)"
# --- the pointer (laser): a red trail, no ink
$ink = Ink; $blue = CountBlue 0 100 2000 1070
UseTool laser
B line 0.2 0.45 0.6 0.45 30
$red = Count 236 36 48 0 100 2000 1070
Check ($red -gt 20) "the pointer leaves a red trail ($red px)"
Start-Sleep 2
Check (((Ink) -eq $ink) -and ((CountBlue 0 100 2000 1070) -eq $blue)) "and draws nothing"
Check ((Count 236 36 48 0 100 2000 1070) -lt 5) "the trail faded away"
# --- text and shape tools are one press away, + adds a page, full screen toggles
UseTool pen
Press plus; Start-Sleep 1.5
Check ((Title) -match "page 2/2") "the + button added a page ($(Title))"
Press full; Start-Sleep 1; Press full; Start-Sleep 1
Check ((Alive)) "full screen on and off"
# --- undo everything
for ($k = 0; $k -lt 8; $k++) { Press undo }
Wheel 10
"errors: " + (Errs); "alive: " + (Alive)
Shot "s5-end" | Out-Null
Stop-App
