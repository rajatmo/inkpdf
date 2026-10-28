# A seeded storm of real input over every tool: stylus (tip, barrel), mouse, finger, wheel, keyboard and the
# toolbar's undo/redo, in random order, on one document. The app must stay alive, print no errors, and keep
# answering (a screenshot still shows the page) after every round.
param([int]$Seed = 42, [int]$Rounds = 30)
. "$PSScriptRoot\lib.ps1"
$script:UseMouse = $true
$rng = New-Object Random $Seed
function R([double]$a, [double]$b) { [math]::Round($a + ($b - $a) * $rng.NextDouble(), 3) }
function Pick($items) { $items[$rng.Next($items.Count)] }
Launch "tool pen`n" "" "x"
$words = @("alpha", "beta gamma", "x", "line one", "ünï", "12345", "a b c d e f")
$acts = @(
    { B stroke (R 0.15 0.5) (R 0.2 0.8) (R 0.5 0.85) (R 0.2 0.8) (R 0.005 0.03) (R 1 4) 40 },
    { B line (R 0.15 0.5) (R 0.2 0.8) (R 0.5 0.85) (R 0.2 0.8) 20 },
    { B tap (R 0.15 0.85) (R 0.2 0.85) },
    { $x = R 0.2 0.8; $y = R 0.2 0.8; B tap $x $y tap $x $y },
    { Lasso (R 0.1 0.4) (R 0.2 0.5) (R 0.5 0.9) (R 0.5 0.85) },
    { B flag barrel on tap (R 0.15 0.85) (R 0.2 0.85) },
    { B rect (R 0.2 0.4) (R 0.3 0.5) (R 0.5 0.7) (R 0.55 0.8) 6 },
    { B circle (R 0.3 0.7) (R 0.4 0.6) (R 0.05 0.12) 24 },
    { Mouse-Click (R 0.1 0.9) (R 0.15 0.9) },
    { Mouse-Drag (R 0.1 0.9) (R 0.15 0.9) (R 0.1 0.9) (R 0.15 0.9) },
    { Wheel (Pick @(-3, -1, 2, 4)) },
    { Type-Text (Pick $words) },
    { Keys (Pick @("{ENTER}", "{BACKSPACE}", "{ESC}")) },
    { B drag 0.5 0.7 0.5 0.4 15 },
    { B pinch 0.5 0.5 (R 0.04 0.1) (R 0.12 0.3) 15 },
    { Press undo },
    { Press redo }
)
$tools = @("pen", "shape", "text", "laser")
$done = 0
for ($round = 1; $round -le $Rounds; $round++) {
    $tool = Pick $tools
    UseTool $tool
    for ($i = 0; $i -lt 4; $i++) { & (Pick $acts) }
    if (-not (Alive)) { Fail "the app exited in round $round (tool $tool)"; break }
    $done = $round
}
Start-Sleep -Milliseconds 500
Check (Alive) "alive after $done rounds"
$shot = Shot "r2-final"
Check ((Get-Item $shot).Length -gt 20000) "the window still renders the page"
$e = @(Errs)
Check ($e.Count -eq 0) "no errors or panics printed ($($e -join '; '))"
"rounds: $done, seed: $Seed"
Stop-App
