. "$PSScriptRoot\lib.ps1"
# 5. AXES
Launch "tool geometry`nshape axes2d`nshape-slot 0`n" "" "x"
Drag 0.2 0.3 0.6 0.7 12
Snap
Check (((Straight 400 535 1200 535) -ge 0.95) -and ((Straight 800 321 800 749) -ge 0.95)) "two straight axes crossing at the origin"
Check (((Accent 785 520 815 550) -gt 8) -and ((Accent 385 520 415 550) -gt 8) -and ((Accent 1185 520 1215 550) -gt 8) -and ((Accent 785 306 815 336) -gt 8) -and ((Accent 785 734 815 764) -gt 8)) "anchors at the origin and the four ends"
# the x tip dragged diagonally slides along the x axis only
Drag 0.6 0.5 0.65 0.5607 10
Snap
Check ((Extent 1250 560 1400 700)[0] -lt 0) "the tip did not leave the x axis"
$e = Extent 1150 515 1400 555
Check ((Extent 1150 515 1400 555)[2] -ge 1290) "it slid along the axis to about x=1300 ($($e -join ', '))"
Check ((Straight 400 535 1300 535) -ge 0.95) "the x axis is still one straight horizontal line"
# the crossing dragged: the axes move to pass through it
Drag 0.4 0.5 0.45 0.5607 10
Snap
Check (((Straight 400 600 1290 600) -ge 0.95) -and ((Straight 900 321 900 749) -ge 0.95)) "the axes now cross at the new origin, still horizontal and vertical ($(Straight 400 600 1290 600), $(Straight 900 321 900 749))"
Check ((Straight 400 535 1290 535) -lt 0.3) "the old x axis line is gone"
Shot "s4-1-axes2d" | Out-Null

Launch "tool geometry`nshape axes3d`nshape-slot 0`n" "" "x"
Drag 0.2 0.3 0.6 0.7 12
Snap
Check (((Straight 400 535 1200 535) -ge 0.95) -and ((Straight 800 321 800 749) -ge 0.95) -and ((Straight 940 460 520 685) -ge 0.95)) "three straight axes: x, y and the z axis through the origin"
# the z tip dragged off its line slides along the line
Drag 0.26 0.6402 0.26 0.4673 10
Snap
Check ((Straight 940 460 520 685) -ge 0.9 -or (Straight 940 460 600 640) -ge 0.9) "the z axis is still a straight line through the origin ($(Straight 940 460 520 685))"
Check ((Extent 500 470 560 520)[0] -lt 0) "its tip did not leave the line"
Shot "s4-2-axes3d" | Out-Null
"errors: " + (Errs); "alive: " + (Alive)
Stop-App
