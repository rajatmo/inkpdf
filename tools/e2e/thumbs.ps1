# The pictures of the pages in the overview show what is drawn on them: on a blank document, with a page added and ink on each of
# the three pages (pages that were added have no picture from the file, only what is on them), and typed text on the last.
. "$PSScriptRoot\lib.ps1"
# The three pictures, as fractions of a maximised 4K window's 2000 x 1070 units, a little inside their edges (the ring round
# the page being looked at is the interface's blue, near the ink's).
$thumbs = @(@(560, 168, 645, 213), @(676, 168, 761, 213), @(792, 168, 877, 213))
function Ink-In($n) { $t = $thumbs[$n - 1]; Count 32 96 224 $t[0] $t[1] $t[2] $t[3] }

Launch "tool pen`nslot 1`nwidth 3.50`ntext-slot 0`ntext-size 40.00`n" "" @("--mode", "desktop")
Start-Sleep -Milliseconds 800
B stroke 0.10 0.30 0.50 0.30 0.075 2.5 150; Start-Sleep -Milliseconds 300
Press plus; Start-Sleep -Milliseconds 1200
B stroke 0.10 0.45 0.50 0.45 0.04 5 150; Start-Sleep -Milliseconds 300
Press plus; Start-Sleep -Milliseconds 1200
B stroke 0.10 0.55 0.60 0.40 0.05 3 150; Start-Sleep -Milliseconds 300
Press pages
Start-Sleep -Milliseconds 2500
Check (((Pages)[1] -eq 3)) "a blank document with two pages added has three ($(Title))"
Check (((Ink-In 1) -gt 8) -and ((Ink-In 2) -gt 8) -and ((Ink-In 3) -gt 8)) "each picture has its page's ink, the added ones too ($(Ink-In 1), $(Ink-In 2), $(Ink-In 3))"
Press pages
Check ((Alive) -and ((Errs).Count -eq 0)) "no crash, nothing printed to stderr"
Stop-App
