# Runs the real-input checks against the release build (`cargo build --release -p inkpdf`) and says what passed.
# They drive the REAL app with a synthetic stylus and finger, the real mouse and keyboard, so they take over both for a
# minute or two and need an unlocked desktop: do not touch the machine while they run.
#   .\tools\e2e\run.ps1             everything
#   .\tools\e2e\run.ps1 -Only smoke just that script
param([string[]]$Only)
$scripts = if ($Only) { $Only } else { "smoke", "modes", "overview", "thumbs" }
$clock = [Diagnostics.Stopwatch]::StartNew()
$passed = 0; $failed = 0
foreach ($s in $scripts) {
    $text = & powershell -NoProfile -File (Join-Path $PSScriptRoot "$s.ps1") 2>&1 | Out-String
    $ok = ([regex]::Matches($text, "(?m)^ok:")).Count
    $bad = ([regex]::Matches($text, "(?m)^FAIL:")).Count
    $crashed = $text -match "exception|cannot find|is not recognized"
    $passed += $ok; $failed += $bad + [int]$crashed
    "{0,-6} {1,2} ok, {2} failed{3}" -f $s, $ok, $bad, $(if ($crashed) { ", THE SCRIPT ITSELF FAILED" } else { "" })
    $text -split "`n" | Where-Object { $_ -match "^FAIL:|exception|cannot find|is not recognized" } | ForEach-Object { "       $($_.TrimEnd())" }
}
"total: $passed ok, $failed failed, in {0:N0} s" -f $clock.Elapsed.TotalSeconds
exit ([int]($failed -gt 0))


