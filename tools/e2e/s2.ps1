. "$PSScriptRoot\lib.ps1"
# 2. SPLINES: endpoints, tangents, node positions and tangents
Launch "tool geometry`nshape curve`nshape-slot 0`n" "" "x"
Drag 0.2 0.6 0.6 0.6 10
Snap
Check ((Straight 400 642 1200 642) -lt 0.4) "a spline is not straight"
Check (((Extent 405 635 450 680)[0] -ge 0) -and ((Extent 1150 600 1195 650)[0] -ge 0)) "it runs through both end anchors"
Check (((Accent 650 865 690 900) -gt 2) -and ((Accent 915 385 955 420) -gt 2)) "each end shows its tangent handle"
# double tap the middle of the S (it passes through the centre): a node is added without changing the shape
Snap; $b0 = Extent 300 150 1300 1050
Dbl 0.4 0.6
Snap
$b1 = Extent 300 150 1300 1050
Check ((Accent 785 627 815 657) -gt 8) "a node appeared where the curve was tapped"
Check ((Near $b0[1] $b1[1] 4) -and (Near $b0[3] $b1[3] 4)) "the curve did not change (its top and bottom: $($b0[1]),$($b0[3]) -> $($b1[1]),$($b1[3]))"
Check (((Accent 650 685 685 720) -gt 2) -and ((Accent 915 565 950 600) -gt 2)) "the new node has tangent handles of its own"
# a node's tangent dragged: the curve bends there, the node stays
Shot "s2-1-node" | Out-Null
Press undo
Snap
Check ((Accent 785 627 815 657) -lt 4) "undo took the node away"
# the end tangent dragged further down: the curve bends more, the endpoints stay
$low0 = (Extent 300 150 1300 1050)[3]
Drag 0.3335 0.8245 0.3335 0.9178 8
Snap
$low1 = (Extent 300 150 1300 1050)[3]
Check ($low1 -gt $low0 + 15) "dragging a tangent bent the curve more (lowest point $low0 -> $low1)"
Check (((Extent 405 635 450 680)[0] -ge 0) -and ((Extent 1150 600 1195 650)[0] -ge 0)) "the endpoints did not move"
Press undo
# double tap an anchor: its tangent folds away; again, and it is smooth with a tangent along the chord
Dbl 0.2 0.6
Snap
Check ((Accent 650 865 690 900) -lt 4) "double tap on an anchor folded its tangent away"
Dbl 0.2 0.6
Snap
Check ((Accent 650 625 690 660) -gt 2) "and again gave it a tangent along the line to its neighbour"
Shot "s2-2-anchor" | Out-Null
"errors: " + (Errs); "alive: " + (Alive)
Stop-App
