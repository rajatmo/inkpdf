. "$PSScriptRoot\lib.ps1"
Launch "tool pen`nshape rectangle`nshape-slot 1`ntext-slot 2`ntext-size 40.00`n" "" "x"
function Black($x0,$y0,$x1,$y1) { Count 28 30 36 $x0 $y0 $x1 $y1 }
# --- one of each: ink, a rectangle, text
Drag 0.1 0.2 0.3 0.2 16
Check ((Black 190 200 620 230) -gt 400) "pen ink drawn"
UseTool shape
Drag 0.1 0.35 0.3 0.5 10
Check ((CountBlue 190 370 620 540) -gt 400) "shape tool drew a rectangle"
UseTool text
$script:UseMouse = $true
Tap 0.1 0.6; Type-Text "Hello"; Tap 0.9 0.9; Keys "{ESC}"
$script:UseMouse = $false
UseTool pen
Check ((CountRed 190 640 450 700) -gt 300) "text placed"
$ink0 = Black 190 200 620 230; $rect0 = CountBlue 190 370 620 540; $txt0 = CountRed 190 640 450 700
# --- lasso all three with the barrel button, then drag the selection with the tip
Lasso 0.05 0.15 0.35 0.72
Check ((Count 64 132 255 100 150 800 800) -gt 100) "the lasso selected them (selection frame)"
Drag 0.2 0.42 0.6 0.42 12
Check ((Black 990 200 1420 230) -gt $ink0 * 0.8 -and (Black 190 200 620 230) -lt 30) "the ink moved with the selection"
Check ((CountBlue 990 370 1420 540) -gt $rect0 * 0.8 -and (CountBlue 190 370 620 540) -lt 30) "the rectangle moved with the selection"
Check ((CountRed 990 640 1250 700) -gt $txt0 * 0.8 -and (CountRed 190 640 450 700) -lt 30) "the text moved with the selection"
Press undo
Check ((Black 190 200 620 230) -gt $ink0 * 0.8 -and (CountBlue 190 370 620 540) -gt $rect0 * 0.8 -and (CountRed 190 640 450 700) -gt $txt0 * 0.8) "one undo put all three back"
Press redo
Check ((Black 990 200 1420 230) -gt $ink0 * 0.8 -and (CountRed 990 640 1250 700) -gt $txt0 * 0.8) "redo moved them again"
Press undo
# --- let go, then edit the rectangle's anchors with the shape tool
UseTool shape
Tap 0.8 0.9
Tap 0.2 0.35
Check ((Count 64 132 255 190 350 640 560) -gt 80) "tapping the rectangle's outline selected it (anchors show)"
Drag 0.3 0.5 0.45 0.65 10
Check ((CountBlue 660 560 910 700) -gt 100) "dragging a corner anchor reshaped the rectangle"
Shot "a8-1-reshaped" | Out-Null
Type-Text "ignored"
Check ((CountRed 190 640 450 700) -gt $txt0 * 0.8) "keys in the shape tool do nothing (text untouched)"
Press undo
Check ((CountBlue 660 560 910 700) -lt 60) "undo puts the corner back"
# --- a second page: draw there, go back and forth, undo across pages
Press plus; Start-Sleep 1.5
UseTool pen
Drag 0.1 0.3 0.4 0.3 12
Check ((Black 190 300 820 340) -gt 300) "ink on the new page"
Wheel 40; Start-Sleep 1
Check ((Black 190 200 620 230) -gt 300 -or (Black 990 200 1420 230) -gt 300) "page 1's ink is still on page 1"
"title: $(Title)"
"errors: " + (Errs); "alive: " + (Alive)
Shot "a8-2-end" | Out-Null
Stop-App
