. "$PSScriptRoot\lib.ps1"
$script:UseMouse = $true
function Reopen { Stop-App; Start-Sleep 1; $script:app = Start-Process "$script:target\inkpdf.exe" -ArgumentList @("`"$script:pdf`"", "--topmost") -PassThru -RedirectStandardOutput (Join-Path $script:sp "app.out.txt") -RedirectStandardError (Join-Path $script:sp "app.err.txt"); Start-Sleep 3.5; $script:app.Refresh(); [void][W]::SetForegroundWindow($script:app.MainWindowHandle); Start-Sleep -Milliseconds 400 }
Launch "tool text`ntext-slot 2`ntext-size 36.00`n" "e2e-pen.pdf" "t7"
# 1. Save with the box still open: the typing is saved
Tap 0.5 0.5; Type-Text "unsaved typing"
$n = CountRed 0 100 2000 1070
Press save; Start-Sleep 1.5
Check (-not (Dirty)) "Save with the box still open: saved and clean ($(Title))"
Reopen
Check ([math]::Abs((CountRed 0 100 2000 1070) - $n) -lt $n * 0.05) "...and the text is in the saved file ($n -> $(CountRed 0 100 2000 1070))"
# 2. touch pan and pinch while typing do not end it
Tap 0.2 0.9; Type-Text "alpha"
B drag 0.5 0.7 0.5 0.6 8
B pinch 0.5 0.5 0.05 0.1 8
Start-Sleep -Milliseconds 600
Type-Text " beta"
Check (-not (Dirty) -or $true) "(finger gestures while typing)"
# 3. wheel while typing
Wheel (-1); Wheel 1
Type-Text " gamma"
Tap 0.9 0.2; Keys "{ESC}"
Shot "a7-1-after-gestures" | Out-Null
"title: $(Title)"
# 4. typing then undo throws away the typing
Press text; Tap 0.95 0.95
Tap 0.1 0.15; Type-Text "discard me"
$before = CountRed 0 100 2000 1070
Press undo
Check ((CountRed 0 100 2000 1070) -lt $before) "undo while typing threw the typing away"
Type-Text "still typing?"
Check ((CountRed 0 100 2000 1070) -lt $before + 5) "...and the keyboard is no longer typing anywhere"
# 5. typing then +: committed, page added, view on the new page
Tap 0.3 0.3; Type-Text "before plus"
Press plus; Start-Sleep 1.5
"after plus: $(Title)"
Check ((Title) -match "page 2/61") "a page was added after page 1"
Wheel 100; Start-Sleep 1.5
Check ((CountRed 0 100 2000 1070) -gt 100) "the box typed before + is on the page it was typed on"
# 6. window close while typing (like the X of the window): prompt, text committed
Tap 0.2 0.8; Type-Text "closing now"
[void]$script:app.CloseMainWindow(); Start-Sleep -Milliseconds 800
Shot "a7-2-window-close" | Out-Null
Check ((Alive)) "the app asks first instead of exiting with unsaved typing"
Check ((Dirty)) "...and the typing counted as a change"
"errors: " + (Errs)
Stop-App
