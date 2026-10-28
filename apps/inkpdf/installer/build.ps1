# Builds the release exe and the Explorer thumbnail handler, and wraps them in InkPDF-Setup-<version>.exe.
# Nothing is written inside the repository: the build goes to `inkpdf-build` and the installer to `inkpdf-dist`,
# both beside the repository (set CARGO_TARGET_DIR and INKPDF_DIST to put them elsewhere).
# Needs Inno Setup 6 (`winget install JRSoftware.InnoSetup`). Run from anywhere.
param([switch]$SkipBuild)
# cargo and ISCC write progress to stderr, so judge them by exit code, not by PowerShell errors.
$ErrorActionPreference = "Continue"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$crate = Split-Path -Parent $here
$repo = Split-Path -Parent (Split-Path -Parent $crate)
$workspace = Split-Path -Parent $repo
$version = (Select-String -Path (Join-Path $crate "Cargo.toml") -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $workspace "inkpdf-build" }
$dist = if ($env:INKPDF_DIST) { $env:INKPDF_DIST } else { Join-Path $workspace "inkpdf-dist" }
New-Item -ItemType Directory -Force $dist | Out-Null
if (-not $SkipBuild) {
    Push-Location $crate
    # (Put back afterwards, so running this does not change the build folder of what is run next in this shell.)
    $oldTarget = $env:CARGO_TARGET_DIR
    $env:CARGO_TARGET_DIR = $targetDir
    try { & cargo build --release -p inkpdf -p inkpdf-thumb; if ($LASTEXITCODE -ne 0) { throw "cargo build failed" } }
    finally {
        Pop-Location
        if ($oldTarget) { $env:CARGO_TARGET_DIR = $oldTarget } else { Remove-Item Env:\CARGO_TARGET_DIR -ErrorAction SilentlyContinue }
    }
}
$exe = Join-Path $targetDir "release\inkpdf.exe"
if (-not (Test-Path $exe)) { throw "no release build at $exe" }
$thumb = Join-Path $targetDir "release\inkpdf_thumb.dll"
if (-not (Test-Path $thumb)) { throw "no thumbnail handler build at $thumb" }
$iscc = @("$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe", "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) { throw "Inno Setup 6 not found: winget install JRSoftware.InnoSetup" }
& $iscc "/DAppVersion=$version" "/DExeSource=$exe" "/DThumbSource=$thumb" "/O$dist" (Join-Path $here "inkpdf.iss")
if ($LASTEXITCODE -ne 0) { throw "Inno Setup failed" }
$out = Join-Path $dist "InkPDF-Setup-$version.exe"
"built $out ($([math]::Round((Get-Item $out).Length / 1MB, 1)) MB)"
