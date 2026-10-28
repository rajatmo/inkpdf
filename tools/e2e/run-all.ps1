# Runs every real-input check against the release build and prints one line per script.
# These drive the REAL app with a synthetic stylus and finger (inkpdf-bot), the real mouse and keyboard, so
# they take over the mouse and keyboard for several minutes and need an unlocked desktop: do not touch the
# machine while they run. Build first: cargo build --release -p inkpdf
#   .\tools\e2e\run-all.ps1            all of them
#   .\tools\e2e\run-all.ps1 -Only r1   just r1
param([string[]]$Only)
$all = "a1", "a2", "a3", "a4", "a5", "a6", "a7", "a8", "s1", "s2", "s3", "s4", "s5", "t1", "r1", "r2", "r3", "p1"
$scripts = if ($Only) { $Only } else { $all }
$failed = 0; $passed = 0
foreach ($s in $scripts) {
    $text = & powershell -NoProfile -File (Join-Path $PSScriptRoot "$s.ps1") 2>&1 | Out-String
    $ok = ([regex]::Matches($text, "(?m)^ok:")).Count
    $bad = ([regex]::Matches($text, "(?m)^FAIL:")).Count
    $crashed = $text -match "alive: False|panicked"
    $passed += $ok; $failed += $bad + [int]$crashed
    "{0,-4} {1,3} ok, {2} failed{3}" -f $s, $ok, $bad, $(if ($crashed) { ", APP CRASHED" } else { "" })
    $text -split "`n" | Where-Object { $_ -match "^FAIL:" } | ForEach-Object { "     $_" }
}
"total: $passed ok, $failed failed"
exit ([int]($failed -gt 0))