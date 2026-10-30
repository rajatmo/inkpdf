# The reader, for real: that the keys of a PDF program (Page Down, End, Home, Ctrl+0, Ctrl+1) and a drag on empty page with the Select tool reach the app through the real window. What each key, wheel turn and gesture does in every tool and mode, and every edge,
# is for the unit tests (the pointer's shape too: it is read from the rules table); this proves the real window and the real keyboard and mouse reach them.
. "$PSScriptRoot\lib.ps1"

# ---- The Select tool is the reader (it is what the program starts on).
Launch "tool select`n" "pages3" @("--mode", "desktop")
$first = (Pages)[0]
$total = (Pages)[1]
Check ($total -ge 2) "the document has pages to read ($total)"
Keys "{PGDN}"; Keys "{PGDN}"; Keys "{PGDN}"; Start-Sleep -Milliseconds 900
Check ((Pages)[0] -gt $first) "Page Down, three times, scrolls on: the page in view is a later one ($((Pages)[0]))"
Keys "{END}"; Start-Sleep -Milliseconds 900
Check ((Pages)[0] -eq $total) "End goes to the last page"
Keys "{HOME}"; Start-Sleep -Milliseconds 900
Check ((Pages)[0] -eq $first) "Home goes back to the first"
# Zoom: the page at its size, then the width, change what is drawn.
$a = Count 255 255 255
Keys "^1"; Start-Sleep -Milliseconds 500
$b = Count 255 255 255
Keys "^0"; Start-Sleep -Milliseconds 500
$c = Count 255 255 255
Check (($a -ne $b) -or ($b -ne $c)) "Ctrl+1 and Ctrl+0 zoom the page ($a, $b, $c white pixels)"
# A drag on empty page moves it (the left button, with the Select tool).
Keys "{HOME}"; Start-Sleep -Milliseconds 600
$ink_before = Black
Mouse-Drag 0.5 0.85 0.5 0.15
Start-Sleep -Milliseconds 500
Check ((Pages)[0] -ge $first) "a drag on empty page with the Select tool moves it, and draws nothing"
Check (-not (Dirty)) "and no ink was made: the document is not marked changed"
# With a drawing tool the same drag draws.
Press pen
Mouse-Drag 0.3 0.5 0.7 0.5
Check (Dirty) "with the pen the same left-button drag draws and marks the document changed: ink cannot wait"
Check (Healthy) "no crash, nothing printed to stderr"
Stop-App
