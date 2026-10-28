# Builds a standalone, portable InkPDF: one exe with the C runtime linked in (nothing to install, no Visual C++
# redistributable needed), plus a portable.txt marker so its settings are kept beside it, zipped as
# InkPDF-Portable-<version>.zip (and left unpacked in InkPDF-Portable-<version>\) in `inkpdf-dist`.
# Nothing is written inside the repository: it is built into its own folder, `inkpdf-build-portable` (so it does not
# disturb normal builds), and the package goes to `inkpdf-dist`, both beside the repository (set CARGO_TARGET_DIR,
# which gets "-portable" added, and INKPDF_DIST to put them elsewhere). Run from anywhere.
param([switch]$SkipBuild)
$ErrorActionPreference = "Continue"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$crate = Split-Path -Parent $here
$repo = Split-Path -Parent (Split-Path -Parent $crate)
$workspace = Split-Path -Parent $repo
$version = (Select-String -Path (Join-Path $crate "Cargo.toml") -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$normalTarget = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR.TrimEnd('\') } else { Join-Path $workspace "inkpdf-build" }
$targetDir = "$normalTarget-portable"
$dist = if ($env:INKPDF_DIST) { $env:INKPDF_DIST } else { Join-Path $workspace "inkpdf-dist" }
if (-not $SkipBuild) {
    # (Put back afterwards: a later ordinary build in this session must not be built into the portable folder, or with
    # the C runtime linked in.)
    $oldTarget = $env:CARGO_TARGET_DIR; $oldFlags = $env:RUSTFLAGS
    $env:CARGO_TARGET_DIR = $targetDir
    $env:RUSTFLAGS = "-C target-feature=+crt-static"
    Push-Location $crate
    try { & cargo build --release -p inkpdf; if ($LASTEXITCODE -ne 0) { throw "cargo build failed" } }
    finally {
        Pop-Location
        $env:CARGO_TARGET_DIR = $oldTarget; $env:RUSTFLAGS = $oldFlags
        if (-not $oldTarget) { Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue }
        if (-not $oldFlags) { Remove-Item Env:\RUSTFLAGS -ErrorAction SilentlyContinue }
    }
}
$exe = Join-Path $targetDir "release\inkpdf.exe"
if (-not (Test-Path $exe)) { throw "no portable build at $exe" }
$out = $dist
New-Item -ItemType Directory -Force $out | Out-Null
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