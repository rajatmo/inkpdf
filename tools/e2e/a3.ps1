. "$PSScriptRoot\lib.ps1"
# Text boxes among the other tools: lasso and move by the pen tool, ink over text, full screen, zoom
Launch "tool text`ntext-slot 2`ntext-size 40.00`n" "" "x"
function Old { CountRed 590 420 800 490 }
function New1 { CountRed 1190 690 1420 760 }
$script:UseMouse = $true
Tap 0.3 0.4; Type-Text "Move me"; Tap 0.8 0.85; Keys "{ESC}"
$script:UseMouse = $false
$orig = Old
Check ($orig -gt 300) "box placed with the mouse ($orig px)"
UseTool pen
Lasso 0.28 0.38 0.45 0.48
Check ((Count 64 132 255 500 380 1000 520) -gt 40) "the barrel-button lasso selected the text box (a frame shows)"
Drag 0.31 0.42 0.61 0.67 12
Check (((New1) -gt 300) -and ((Old) -lt 40)) "the pen tool dragged the selected box ($(New1) px at the new place)"
Press undo
Check (((Old) -gt 300) -and ((New1) -lt 40)) "undo put it back"
Press redo
Check ((New1) -gt 300) "redo moved it again"
Tap 0.9 0.95
Drag 0.55 0.66 0.8 0.7 20
Check ((Count 28 30 36 1100 650 1700 800) -gt 100) "ink is drawn over the text"
Check ((New1) -gt 300) "the text is still there under the ink"
Press full; Start-Sleep 1
Check ((CountRed 0 100 2000 1070) -gt 300) "text still shown in full screen"
Press full; Start-Sleep 1
Check ((New1) -gt 300) "...and after leaving full screen"
$z0 = CountRed 0 100 2000 1070
B pinch 0.6 0.66 0.05 0.2 12
Start-Sleep -Milliseconds 800
$z1 = CountRed 0 100 2000 1070
Check ($z1 -gt $z0 * 3) "pinch zoom grows the text with the page ($z0 -> $z1 red px)"
"errors: " + (Errs); "alive: " + (Alive)
Stop-App