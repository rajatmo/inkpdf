. "$PSScriptRoot\lib.ps1"
$script:UseMouse = $true
Launch "tool text`ntext-slot 2`ntext-size 40.00`n" "" "x"
Tap 0.3 0.4; Type-Text "keep me"
Check (-not (Dirty)) "typing, nothing committed yet"
Press close; Start-Sleep -Milliseconds 700
Shot "a5-1-prompt" | Out-Null
Check ((Dirty)) "closing committed the typing, so the document counts as changed"
"title: $(Title)"
$before = Count 160 40 50 0 100 2000 1070
# under the prompt: keyboard, mouse, pen on the page and the wheel do nothing to the document
Type-Text "zzz"; Keys "{ENTER}{BACKSPACE}{ESC}"
Mouse-Click 0.7 0.7; Wheel 2
Tap 0.2 0.8; Type-Text "yyy"
Check ((Count 160 40 50 0 100 2000 1070) -eq $before) "under the prompt nothing on the page changed"
# Cancel (the arrow): the prompt goes, the text stays
Tap 0.5 0.54; Start-Sleep -Milliseconds 400
Check ((CountRed 560 410 800 500) -gt 300) "Cancel keeps the text"
$kept = CountRed 300 150 1900 1000
Type-Text "after cancel"
Check ((CountRed 300 150 1900 1000) -eq $kept) "after Cancel the keyboard still types nothing (the text is not being edited)"
Shot "a5-2-cancelled" | Out-Null
# close again and Discard (the bin)
Press close; Start-Sleep -Milliseconds 600
Tap 0.4655 0.54; Start-Sleep -Milliseconds 800
Check ((CountRed 300 150 1900 1000) -eq 0) "Discard gives a blank page"
Check (-not (Dirty)) "...that is not marked changed"
# and typing in the fresh page works
Tap 0.3 0.4; Type-Text "fresh"
Check ((CountRed 560 410 900 500) -gt 200) "a new document takes text"
"errors: " + (Errs); "alive: " + (Alive)
Stop-App
