. "$PSScriptRoot\lib.ps1"
# 3. RECTANGLES and 4. ELLIPSES
Launch "tool geometry`nshape rectangle`nshape-slot 0`n" "" "x"
Drag 0.2 0.3 0.5 0.6 10
Snap
Check (((Straight 400 321 1000 321) -ge 0.95) -and ((Straight 1000 321 1000 642) -ge 0.95) -and ((Straight 1000 642 400 642) -ge 0.95) -and ((Straight 400 642 400 321) -ge 0.95)) "a rectangle: four straight, axis-aligned sides"
Check (((Accent 385 306 415 336) -gt 8) -and ((Accent 985 627 1015 657) -gt 8)) "anchors at the two ends of the diagonal"
Check (((Accent 985 306 1015 336) -lt 4) -and ((Accent 385 627 415 657) -lt 4)) "and no anchors at the other two corners"
Check ((Straight 400 321 1000 642 150 185 255) -gt 0.25) "the diagonal is shown as a dashed guide"
# drag the bottom-right anchor: still a rectangle
Drag 0.5 0.6 0.6 0.7 10
Snap
$e = Extent 300 150 1500 1000
Check ((Near $e[0] 400) -and (Near $e[1] 321) -and (Near $e[2] 1200) -and (Near $e[3] 749)) "dragging the diagonal's end resized it to the new box ($($e -join ', '))"
Check (((Straight 1200 321 1200 749) -ge 0.95) -and ((Straight 400 749 1200 749) -ge 0.95)) "the new sides are straight"
# a double tap on an edge and the barrel on an anchor change nothing
Dbl 0.3 0.3
BarrelTap 0.6 0.7
Snap
$e2 = Extent 300 150 1500 1000
Check ((Near $e2[2] 1200) -and (Near $e2[3] 749) -and ($e2[0] -ge 0)) "no node can be added or taken away from a rectangle"
# dragging the first anchor past the other end flips it: still a rectangle
Drag 0.2 0.3 0.7 0.8 10
Snap
$e3 = Extent 300 150 1500 1000
Check ((Near $e3[0] 1200) -and (Near $e3[1] 749) -and (Near $e3[2] 1400) -and (Near $e3[3] 856)) "dragged past the opposite corner it is the box between them ($($e3 -join ', '))"
Press undo; Press undo
Snap
$e4 = Extent 300 150 1500 1000
Check ((Near $e4[0] 400) -and (Near $e4[2] 1000)) "two undos put it back to the first rectangle"

# --- ellipses
Launch "tool geometry`nshape ellipse`nshape-slot 0`n" "" "x"
Drag 0.2 0.3 0.5 0.6 12
Snap
$e = Extent 300 150 1300 800
Check ((Near $e[0] 400 5) -and (Near $e[1] 321 5) -and (Near $e[2] 1000 5) -and (Near $e[3] 642 5)) "an ellipse fills its bounding box ($($e -join ', '))"
Check (((Accent 385 306 415 336) -gt 8) -and ((Accent 985 306 1015 336) -gt 8) -and ((Accent 985 627 1015 657) -gt 8) -and ((Accent 385 627 415 657) -gt 8)) "anchors at the four corners of the bounding box"
Check ((Straight 400 321 1000 321 150 185 255) -gt 0.25) "the bounding box is shown as dashed guides"
Check ((Extent 385 306 415 336)[0] -lt 0) "the curve itself does not reach the corners"
# drag the top-right corner out: the opposite corner stays, the ellipse grows to fill the new box
Drag 0.5 0.3 0.6 0.2 12
Snap
$e = Extent 300 100 1400 800
Check ((Near $e[0] 400 5) -and (Near $e[1] 214 5) -and (Near $e[2] 1200 5) -and (Near $e[3] 642 5)) "dragging a corner reshaped the ellipse to the new box ($($e -join ', '))"
Check ((Extent 385 200 415 230)[0] -lt 0) "and it is still round (nothing at the box's corner)"
Shot "s3-1-ellipse" | Out-Null
"errors: " + (Errs); "alive: " + (Alive)
Stop-App
