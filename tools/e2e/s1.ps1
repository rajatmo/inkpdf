. "$PSScriptRoot\lib.ps1"
# 1. STRAIGHT LINES: nodes, broken lines, polygons, snapping
Launch "tool geometry`nshape line`nshape-slot 0`n" "" "x"
Drag 0.2 0.5 0.6 0.5 10
Snap
Check ((Straight 400 535 1200 535) -ge 0.95) "a line is drawn straight"
Check (((Accent 385 520 415 550) -gt 10) -and ((Accent 1185 520 1215 550) -gt 10)) "anchors show at both ends"
# a node by double-tapping the line, then pulled down a little: every segment must stay straight
Dbl 0.4 0.5
Snap; Check ((Accent 785 520 815 550) -gt 10) "a node was added in the middle"
Check ((Straight 400 535 1200 535) -ge 0.95) "...without changing the line"
Drag 0.4 0.5 0.4 0.56 8
Snap
Check ((Straight 400 535 800 599) -ge 0.95) "end to node: still straight after the node was moved a little ($(Straight 400 535 800 599))"
Check ((Straight 800 599 1200 535) -ge 0.95) "node to end: still straight ($(Straight 800 599 1200 535))"
Shot "s1-1-broken" | Out-Null
# a fourth node on the second segment, pulled up
Dbl 0.5 0.53
Drag 0.5 0.53 0.5 0.42 8
Snap
Check ((Straight 800 599 1000 449) -ge 0.95) "node to the new node: straight ($(Straight 800 599 1000 449))"
Check ((Straight 1000 449 1200 535) -ge 0.95) "new node to the end: straight ($(Straight 1000 449 1200 535))"
# the last end dragged onto the first end: they snap and the broken line closes into a polygon
Drag 0.6 0.5 0.201 0.5 12
Snap
Check ((Straight 1000 449 400 535) -ge 0.95) "the polygon's closing side is a straight line ($(Straight 1000 449 400 535))"
Check ((Straight 400 535 800 599) -ge 0.95 -and (Straight 800 599 1000 449) -ge 0.95) "and its other sides"
Check ((Extent 1180 515 1230 555)[0] -lt 0) "the end that was dragged is gone: the two ends became one node"
Shot "s1-2-polygon" | Out-Null
Press undo
Snap
Check ((Extent 1180 515 1230 555)[0] -ge 0) "undo opens the polygon again (the end is back)"
Press redo
# let go, then a separate line extended by the barrel button on empty page
UseTool pen; UseTool shape
Drag 0.1 0.8 0.2 0.8 8
BarrelTap 0.3 0.9
Snap
Check ((Straight 200 856 400 856) -ge 0.95) "the first segment of the second line is straight"
Check ((Straight 400 856 600 963) -ge 0.95) "a barrel tap on empty page added a node, joined by a straight segment ($(Straight 400 856 600 963))"
# double tap on its first node: it closes into a triangle
Dbl 0.1 0.8
Snap
Check ((Straight 600 963 200 856) -ge 0.95) "a double tap on an end node closed the triangle ($(Straight 600 963 200 856))"
Shot "s1-3-triangle" | Out-Null
Press undo
Snap
Check ((Straight 600 963 200 856) -lt 0.4) "undo takes the closing side away"
"errors: " + (Errs); "alive: " + (Alive)
Stop-App
