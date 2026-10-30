# The pages, for real: delete and add them with every device, save and reopen; the pages overview opens, its pictures show the ink
# (added pages too), a page or a contents entry goes there, edit mode duplicates, moves (a real drag) and deletes, each undoes,
# and the file keeps the new order. The rules for every device and edge are the unit and storm tests'.
. "$PSScriptRoot\lib.ps1"
# Red ink inside the picture of page `n` in the overview.
function Red-In($n) { $p = Spot overview/pages "$n"; Count-Near $script:red $p ([int]($p[2] / 2 - 4)) ([int]($p[3] / 2 - 4)) }
function Page-Click($n) { Click-Spot (Spot overview/pages "$n") }
function Control($name) { Click-Spot (Spot overview/controls $name) }
function Action($name) { Click-Spot (Spot overview/actions $name) }

# ---- 1. Three pages: the bin and + with the mouse, the stylus and a finger; save; reopen.
Launch "tool pen`nslot 2`nwidth 6.00`n" "pages3" @("--mode", "desktop")
Check (((Pages)[1] -eq 3) -and (-not (Dirty))) "a three-page PDF opens ($(Title))"
B stroke 0.3 0.5 0.7 0.5 0.03 2 40
Check ((Red) -gt 500) "ink on page 1"
Press del
Check (((Pages)[1] -eq 2) -and ((Red) -eq 0)) "the bin deletes the page with one press, and its ink goes with it ($(Title))"
Press plus
Check ((Pages)[1] -eq 3) "+ adds a page ($(Title))"
Pen-Press del
Check ((Pages)[1] -eq 2) "the stylus presses the same button"
Start-Sleep -Milliseconds 700   # (a finger is ignored while the pen was just near)
Finger-Press del
Check ((Pages)[1] -eq 1) "and a finger"
Press del
Check ((Pages)[1] -eq 1) "the last page there is stays"
Press save
Check (-not (Dirty)) "save writes the file ($(Title))"
Reopen
Check ((Pages)[1] -eq 1) "reopened: the file has the one page that is left"

# ---- 2. Twelve pages with a table of contents: the overview.
Launch "tool pen`nslot 2`nwidth 6.00`n" "outline12" @("--mode", "desktop")
Check (((Pages)[1] -eq 12) -and ((Pages)[0] -eq 1)) "a twelve-page PDF with a table of contents opens ($(Title))"
Mouse-Drag 0.3 0.4 0.6 0.5
Press pages
Start-Sleep -Milliseconds 1200
Check (((Red-In 1) -gt 130) -and ((Red-In 2) -lt 110)) "the overview opens: the picture of page 1 has its red ink, page 2's has none ($(Red-In 1), $(Red-In 2))"
Page-Click 8
Start-Sleep -Milliseconds 800
Check ((Pages)[0] -eq 8) "a click on a page goes to it and closes the overview ($(Title))"
Press pages
Start-Sleep -Milliseconds 800
Click-Spot (Spot overview/toc "Chapter 2")
Start-Sleep -Milliseconds 800
Check ((Pages)[0] -eq 4) "a click on a contents entry goes to its page ($(Title))"
Press pages
Start-Sleep -Milliseconds 800
Control edit
Page-Click 1; Page-Click 2
Action duplicate
Start-Sleep -Milliseconds 500
Check ((Pages)[1] -eq 14) "edit mode: two pages chosen and duplicated make fourteen ($(Title))"
Keys "^z"
Start-Sleep -Milliseconds 400
Check ((Pages)[1] -eq 12) "Ctrl+Z undoes the duplicate ($(Title))"
Keys "^y"
Start-Sleep -Milliseconds 400
Check ((Pages)[1] -eq 14) "and Ctrl+Y does it again"
Keys "^z"
Start-Sleep -Milliseconds 400
# A real drag: the first page past the last.
$from = Spot overview/pages "1"; $last = Spot overview/pages "12"
Mouse-Px $from[0] $from[1] ($last[0] + $last[2] * 0.6) $last[1]
Start-Sleep -Milliseconds 600
Check (((Red-In 1) -lt 110) -and ((Red-In 12) -gt 130)) "dragging page 1 past the last moves it there, ink and all ($(Red-In 1), $(Red-In 12))"
Keys "{ESC}"; Keys "{ESC}"
Start-Sleep -Milliseconds 400
Keys "^s"
Start-Sleep -Milliseconds 1500
Reopen
Press pages
Start-Sleep -Milliseconds 1200
Check ((Red-In 12) -gt 130) "saved and reopened: the red page is the last, as the file has it"
Control edit
Page-Click 12
Action delete
Start-Sleep -Milliseconds 500
Check (((Pages)[1] -eq 11) -and ((Red-In 11) -lt 110)) "edit mode: the last page deleted from the bar, its ink with it ($(Title))"
Control undo
Start-Sleep -Milliseconds 700
Check ((Red-In 12) -gt 130) "the overview's undo brings it back with its ink"
Control close
Check (Healthy) "no crash, nothing printed to stderr"

# ---- 3. Pages that were added have no picture from the file: their pictures show what is drawn on them.
Launch "tool pen`nslot 2`nwidth 4.00`n" "" @("--mode", "desktop")
B stroke 0.10 0.30 0.50 0.30 0.075 2.5 150
Press plus; Start-Sleep -Milliseconds 800
B stroke 0.10 0.45 0.50 0.45 0.04 5 150
Press plus; Start-Sleep -Milliseconds 800
B stroke 0.10 0.55 0.60 0.40 0.05 3 150
Press pages
Start-Sleep -Milliseconds 2000
Check ((Pages)[1] -eq 3) "a blank document with two pages added has three ($(Title))"
Check (((Red-In 1) -gt 8) -and ((Red-In 2) -gt 8) -and ((Red-In 3) -gt 8)) "each picture has its page's ink, the added ones too ($(Red-In 1), $(Red-In 2), $(Red-In 3))"
Press pages
Check (Healthy) "no crash, nothing printed to stderr"
Stop-App
