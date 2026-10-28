. "$PSScriptRoot\lib.ps1"
$script:UseMouse = $true
function Reopen { Stop-App; Start-Sleep 1; $script:app = Start-Process "$script:target\inkpdf.exe" -ArgumentList @("`"$script:pdf`"", "--topmost") -PassThru -RedirectStandardOutput (Join-Path $script:sp "app.out.txt") -RedirectStandardError (Join-Path $script:sp "app.err.txt"); Start-Sleep 3.5; $script:app.Refresh(); [void][W]::SetForegroundWindow($script:app.MainWindowHandle); Start-Sleep -Milliseconds 400 }
Launch "tool text`ntext-slot 2`ntext-size 36.00`n" "e2e-pen.pdf" "t4"
"start: $(Title)"
# --- a note on page 1
Tap 0.5 0.5; Type-Text "Page one note"; Keys "{ESC}"
$n1 = CountRed 0 100 2000 1070
Check ($n1 -gt 500) "note on page 1 drawn ($n1 px)"
# --- insert a page after it with the + button, then a note on the new page
Press plus; Start-Sleep 1.5
Check ((Title) -match "page 2/61") "the + button added page 2 of 61 and went to it ($(Title))"
Check ((CountRed 0 100 2000 1070) -lt 50) "the new page is blank"
Tap 0.3 0.3; Type-Text "Inserted page note"; Keys "{ESC}"
Check ((CountRed 0 100 2000 1070) -gt 500) "note on the inserted page"
# --- scroll back up to page 1: the first note is where it was
Wheel 40; Start-Sleep 1
"after scrolling up: $(Title)"
Check ((Title) -match "page 1/61") "scrolled back to page 1"
Check ((CountRed 0 100 2000 1070) -gt 500) "the page 1 note is shown again ($(CountRed 0 100 2000 1070) px)"
Shot "a4-1-page1" | Out-Null
# --- save with the real toolbar, then reopen the file
Press save; Start-Sleep 1.5
Check (-not (Dirty)) "saved: the changed mark is gone ($(Title))"
Reopen
"reopened: $(Title)"
Check ((Title) -match "/61") "the file reopened with 61 pages"
$r1 = CountRed 0 100 2000 1070
Check ([math]::Abs($r1 - $n1) -lt ($n1 * 0.05)) "the page 1 note is back as it was ($n1 -> $r1)"
"after scrolling: $(Title)"
Shot "a4-2-reopened-p1" | Out-Null
# --- edit the saved note, save again, reopen: changed once, not duplicated
Press text; Tap 0.95 0.95
Tap 0.5 0.5
Type-Text " (v2)"
Tap 0.9 0.9; Keys "{ESC}"
Check (Dirty) "editing a loaded note marks the document changed"
Shot "a4-3-edited" | Out-Null
Press save; Start-Sleep 1.5
Reopen
$e1 = CountRed 0 100 2000 1070
Shot "a4-4-reopened-edited" | Out-Null
Check ($e1 -gt $r1 * 1.2) "the edit was saved ($r1 -> $e1 px)"
"errors: " + (Errs); "alive: " + (Alive)
Stop-App
