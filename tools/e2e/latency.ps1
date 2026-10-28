param([string]$Exe = "", [string]$Tag = "base")
. "$PSScriptRoot\lib.ps1"
if (-not $Exe) { $Exe = "$script:target\inkpdf.exe" }
Get-Process inkpdf -ErrorAction SilentlyContinue | Stop-Process -Force
$env:APPDATA = Join-Path $script:sp "appdata"; Remove-Item "$env:APPDATA\InkPDF" -Recurse -ErrorAction SilentlyContinue
$out = Join-Path $script:sp "lat-$Tag.out.txt"; $err = Join-Path $script:sp "lat-$Tag.err.txt"
$script:app = Start-Process $Exe -ArgumentList @("--topmost","--stats","--exit-after","22") -PassThru -RedirectStandardOutput $out -RedirectStandardError $err
Start-Sleep 3.5
$script:title = "Untitled"
# many pen strokes: long sweeping lines and scribbles, so the pen path is exercised the whole time
$t0 = Get-Date
while (((Get-Date) - $t0).TotalSeconds -lt 14) {
    & $script:bot Untitled stroke 0.1 0.3 0.9 0.3 0.05 4 120 stroke 0.1 0.5 0.9 0.5 0.08 6 120 stroke 0.1 0.7 0.9 0.7 0.04 9 120 | Out-Null
}
$script:app.WaitForExit(15000) | Out-Null
"== $Tag"; Get-Content $out
