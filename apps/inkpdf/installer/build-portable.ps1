# Builds a standalone, portable InkPDF: one exe with the C runtime linked in (nothing to install, no Visual C++
# redistributable needed), plus a portable.txt marker so its settings are kept beside it, zipped as
# installer\Output\InkPDF-Portable-<version>.zip (and left unpacked in installer\Output\InkPDF-Portable-<version>\).
# It is built into its own target folder so it does not disturb normal builds. Run from anywhere.
param([switch]$SkipBuild)
$ErrorActionPreference = "Continue"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$crate = Split-Path -Parent $here
$version = (Select-String -Path (Join-Path $crate "Cargo.toml") -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$workspace = Split-Path -Parent (Split-Path -Parent $crate)
$targetDir = Join-Path $workspace "target-portable"
if (-not $SkipBuild) {
    $env:CARGO_TARGET_DIR = $targetDir
    $env:RUSTFLAGS = "-C target-feature=+crt-static"
    Push-Location $crate
    try { & cargo build --release -p inkpdf; if ($LASTEXITCODE -ne 0) { throw "cargo build failed" } } finally { Pop-Location }
}
$exe = Join-Path $targetDir "release\inkpdf.exe"
if (-not (Test-Path $exe)) { throw "no portable build at $exe" }
$out = Join-Path $here "Output"
$folder = Join-Path $out "InkPDF-Portable-$version"
Remove-Item $folder -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $folder | Out-Null
Copy-Item $exe (Join-Path $folder "InkPDF.exe")
Set-Content (Join-Path $folder "portable.txt") "While this file is beside InkPDF.exe, InkPDF keeps its settings in settings.txt here instead of in %APPDATA%\InkPDF. Delete this file to use the usual place." -Encoding ascii
Copy-Item (Join-Path $here "readme.txt") (Join-Path $folder "readme.txt")
$zip = Join-Path $out "InkPDF-Portable-$version.zip"
Remove-Item $zip -Force -ErrorAction SilentlyContinue
Compress-Archive -Path (Join-Path $folder "*") -DestinationPath $zip
"built $zip ($([math]::Round((Get-Item $zip).Length / 1MB, 1)) MB); exe $([math]::Round((Get-Item (Join-Path $folder 'InkPDF.exe')).Length / 1MB, 1)) MB"
