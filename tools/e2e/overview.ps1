# The pages overview with real input, in one app start: that the button opens it, that the pictures show the ink, that a click on
# a page or on a table-of-contents entry goes there, that edit mode duplicates, deletes and (by a real mouse drag) moves pages,
# that each of it undoes, and that the file is saved in the new order. The rules for every device and every edge are the unit
# and stress tests'; this proves the real window, the GPU layer and the real mouse and keyboard reach and draw it.
. "$PSScriptRoot\lib.ps1"
# Where things are in a maximised 4K window, as fractions of it: the pages are five to a row, the header's buttons, the bar of edit mode.
$cellX = @(0.4260, 0.4840, 0.5420, 0.5995, 0.6570); $cellY = @(0.1780, 0.3450, 0.5110)
function Cell($n) { @($cellX[($n - 1) % 5], $cellY[[math]::Floor(($n - 1) / 5)]) }
function Page-Click($n) { $c = Cell $n; Click $c[0] $c[1] }
$hdr = @{ close = @(0.7146, 0.0613); edit = @(0.6349, 0.0613); add = @(0.6083, 0.0613); undo = @(0.6615, 0.0613) }
function Header($name) { Click $hdr[$name][0] $hdr[$name][1] }
$barY = 0.9358; $bar = @{ all = 0.5063; duplicate = 0.5573; earlier = 0.5849; later = 0.6130; delete = 0.6411 }
function Bar-Action($name) { Click $bar[$name] $barY }
# Red pixels (the ink of the test) in the picture of page $n, in the window's 2000 x 1070 units.
function Red-In($n) { $c = Cell $n; Count 224 52 52 ($c[0] * 2000 - 45) ($c[1] * 1070 - 60) ($c[0] * 2000 + 45) ($c[1] * 1070 + 60) }
function Accent-In-Card { Accent 540 85 1460 1015 }

Launch "tool pen`nslot 2`nwidth 6.00`n" "outline12" @("--mode", "desktop")
Check (((Pages)[1] -eq 12) -and ((Pages)[0] -eq 1)) "a twelve-page PDF with a table of contents opens ($(Title))"
Mouse-Drag 0.3 0.4 0.6 0.5
Check ((Red) -gt 100) "a red stroke on the first page"
Check ((Accent-In-Card) -lt 300) "little in the accent colour where the card will be ($(Accent-In-Card))"

# ---- opening, and what it shows
Press pages
Start-Sleep -Milliseconds 1200
Check ((Accent-In-Card) -gt 400) "the pages button opens the overview: the page being looked at is ringed, its section lit"
Check ((Red-In 1) -gt 130) "the picture of the first page has the red ink on it ($(Red-In 1))"
Check (((Red-In 2) -lt 110) -and ((Red-In 12) -lt 110)) "and the other pages' pictures have none ($(Red-In 2), $(Red-In 12))"
Mouse-Drag 0.85 0.5 0.85 0.5
Check ((Count 255 255 255 540 85 1460 1015) -gt 50000) "a click on the dimmed document puts it away"
Press pages
Start-Sleep -Milliseconds 800

# ---- going to a page and to a chapter
Page-Click 8
Start-Sleep -Milliseconds 800
Check ((Pages)[0] -eq 8) "a click on a page goes to it and closes the overview ($(Title))"
Press pages
Start-Sleep -Milliseconds 800
Click 0.328 0.2103     # the table of contents' third row: Chapter 2, which is on page 4
Start-Sleep -Milliseconds 800
Check ((Pages)[0] -eq 4) "a click on an entry of the table of contents goes to its page ($(Title))"

# ---- edit mode: duplicate and undo
Press pages
Start-Sleep -Milliseconds 800
Header edit
Start-Sleep -Milliseconds 300
Page-Click 1
Page-Click 2
Start-Sleep -Milliseconds 300
Bar-Action duplicate
Start-Sleep -Milliseconds 500
Header close
Start-Sleep -Milliseconds 500
Check ((Pages)[1] -eq 14) "edit mode: two pages chosen and duplicated, the document has fourteen ($(Title))"
Keys "^z"
Start-Sleep -Milliseconds 400
Check ((Pages)[1] -eq 12) "Ctrl+Z undoes the duplicate: twelve again ($(Title))"
Keys "^y"
Start-Sleep -Milliseconds 400
Check ((Pages)[1] -eq 14) "and Ctrl+Y does it again"
Keys "^z"
Start-Sleep -Milliseconds 300

# ---- a real drag: the first page, with the red ink, to the end
Press pages
Start-Sleep -Milliseconds 1000
$from = Cell 1; $to = @(0.5300, 0.5110)
Mouse-Drag $from[0] $from[1] $to[0] $to[1]
Start-Sleep -Milliseconds 600
Check (((Red-In 1) -lt 110) -and ((Red-In 12) -gt 130)) "dragging the first page past the last moves it there, red ink and all ($(Red-In 1), $(Red-In 12))"
Keys "{ESC}"
Start-Sleep -Milliseconds 400
Check ((Pages)[1] -eq 12) "Esc puts the overview away ($(Title))"

# ---- saved in the new order, and a deletion that undoes
Keys "^s"
Start-Sleep -Milliseconds 1500
Reopen
Start-Sleep -Milliseconds 500
Press pages
Start-Sleep -Milliseconds 1200
Check ((Red-In 12) -gt 130) "saved and reopened: the red page is the last one, as the file has it"
Header edit
Start-Sleep -Milliseconds 300
Page-Click 12
Bar-Action delete
Start-Sleep -Milliseconds 500
Check ((Red-In 11) -lt 110 -and (Red-In 12) -lt 110) "edit mode: the last page is chosen and deleted from the bar, red ink with it ($(Red-In 11), $(Red-In 12))"
Header undo
Start-Sleep -Milliseconds 700
Check ((Red-In 12) -gt 130) "undo from the header brings the page back with its ink"
Header close
Check ((Alive) -and ((Errs).Count -eq 0)) "no crash, nothing printed to stderr"
Stop-App


