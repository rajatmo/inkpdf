# The delete-page button, with real input against the real app: it deletes the page being looked at, with one
# press and no question, with the mouse, the stylus and a finger, in both modes; the ink goes with the page and
# the later pages move up; the saved file really lacks the page; the last page there is stays.
. "$PSScriptRoot\lib.ps1"
$script:UseMouse = $true
function Pages { if ((Title) -match "page (\d+)/(\d+)") { @([int]$Matches[1], [int]$Matches[2]) } else { @(0, 0) } }
function Total { (Pages)[1] }
function Here { (Pages)[0] }
function Reopen {
    Stop-App; Start-Sleep 1
    $a = @("`"$script:pdf`"", "--topmost") + @($script:extraArgs)
    $script:app = Start-Process "$script:target\inkpdf.exe" -ArgumentList $a -PassThru -RedirectStandardOutput (Join-Path $script:sp "app.out.txt") -RedirectStandardError (Join-Path $script:sp "app.err.txt")
    Start-Sleep 3.5; $script:app.Refresh(); [void][W]::SetForegroundWindow($script:app.MainWindowHandle); Start-Sleep -Milliseconds 400
}
# A document of three pages, made by the build's own generator.
& "$script:target\inkpdf-gen.exe" (Join-Path $script:sp "three.pdf") 3 | Out-Null

# ---- A long document: notes on pages 1 and 2, delete page 1, save, reopen.
Launch "tool text`ntext-slot 2`ntext-size 36.00`n" "e2e-pen.pdf" "p1"
Check ((Total) -eq 60 -and -not (Dirty)) "starts with 60 pages and nothing changed ($(Title))"
Tap 0.5 0.5; Type-Text "ONE"; Keys "{ESC}"
Press plus; Start-Sleep 1.5
Check ((Total) -eq 61 -and (Here) -eq 2) "the + button added page 2 of 61 and went to it ($(Title))"
Tap 0.3 0.3; Type-Text "TWO"; Keys "{ESC}"
Wheel 400; Start-Sleep 1
Check ((Here) -eq 1) "back on page 1, at the top of it ($(Title))"
$one = CountRed 950 480 1500 700
Check ($one -gt 200) "the note ONE is on page 1 ($one px)"
Press del; Start-Sleep 1.5
Check ((Total) -eq 60) "one press deleted a page, and nothing was asked: $(Title)"
Check ((Here) -eq 1) "the view is on page 1, which is what was page 2"
Check ((CountRed 950 480 1500 700) -lt 30) "ONE went with its page"
Check ((CountRed 560 280 900 500) -gt 200) "TWO is on the page that moved up"
Check (Dirty) "the document is changed"
Press save; Start-Sleep 1.5
Check (-not (Dirty)) "saved ($(Title))"
Reopen
Check ((Total) -eq 60) "the saved file has 60 pages ($(Title))"
Check (((CountRed 560 280 900 500) -gt 200) -and ((CountRed 950 480 1500 700) -lt 30)) "TWO is on page 1 of the file, ONE is gone"
Press del; Start-Sleep 1; Press del; Start-Sleep 1
Check ((Total) -eq 58) "two presses delete two pages ($(Title))"
Stop-App

# ---- A short document: the last page, the only page, the stylus and a finger.
Launch "tool pen`n" "three.pdf" "p2"
Check ((Total) -eq 3) "three pages ($(Title))"
Wheel -80; Start-Sleep 1
Check ((Here) -eq 3) "on the last page ($(Title))"
B tap 0.093 0.031; Start-Sleep 1.2
Check (((Total) -eq 2) -and ((Here) -eq 2)) "the stylus deleted the last page and the view is on the new last one ($(Title))"
Check (Dirty) "and the document changed"
Start-Sleep 0.8
B finger down 1 0.093 0.031; B finger up 1 0.093 0.031; Start-Sleep 1.2
Check ((Total) -eq 1) "a finger deleted a page ($(Title))"
Press del; Start-Sleep 1
Check (((Total) -eq 1) -and (Alive)) "the only page there is stays ($(Title))"
Press save; Start-Sleep 1.5
Reopen
Check ((Total) -eq 1) "the saved file has the one page that is left ($(Title))"
Stop-App

# ---- The tablet mode: the mouse only scrolls, but it still presses the buttons.
$script:extraArgs = @("--mode", "tablet")
Launch "tool pen`n" "three.pdf" "p3"
Press del; Start-Sleep 1.2
Check ((Total) -eq 2) "tablet mode: the mouse pressed the delete button ($(Title))"
Stop-App
$script:extraArgs = @()

# ---- A blank document: add a page, then delete the current one.
Launch "tool pen`n" "" "x"
Press del; Start-Sleep 1
Check (((Total) -eq 1) -and (-not (Dirty))) "one blank page: the button does nothing ($(Title))"
Press plus; Start-Sleep 1.2
Check ((Total) -eq 2) "a page added ($(Title))"
Press del; Start-Sleep 1.2
Check ((Total) -eq 1) "and deleted ($(Title))"
Stop-App
"errors: " + (Errs)
