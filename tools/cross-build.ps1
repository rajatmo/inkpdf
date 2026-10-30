# Builds InkPDF for Windows, Linux and Android on one Windows PC: Windows and Android natively (Android with Android Studio's SDK, NDK
# and Java, tools\android\build-apk.ps1), Linux in WSL (Ubuntu), which tools/linux/setup-dev.sh prepares once. Tests each platform
# the way CI does, and puts the packages in `inkpdf-dist` beside the repository (the Linux tarball in inkpdf-dist\linux).
#   .\tools\cross-build.ps1                       all three
#   .\tools\cross-build.ps1 -Only windows,android  some of them
# Real input on Windows is tools\e2e\run.ps1; the APK can be tried on the PC with Android Studio's emulator (it has an x86_64 build).
param([string[]]$Only = @("windows", "linux", "android"), [string]$Distro = "Ubuntu")
$ErrorActionPreference = "Stop"
# (`powershell -File` hands "windows,android" over as one string.)
$Only = @($Only | ForEach-Object { $_ -split ',' } | ForEach-Object { $_.Trim() })
$repo =(Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$dist = Join-Path (Split-Path -Parent $repo) "inkpdf-dist"
$clock = [Diagnostics.Stopwatch]::StartNew()
$results = @()

# A Windows path as WSL sees it (C:\a b\c -> /mnt/c/a b/c).
function Wsl-Path([string]$p) { "/mnt/" + $p.Substring(0, 1).ToLower() + ($p.Substring(2) -replace '\\', '/') }

# cargo and the others write progress to stderr: judge them by exit code, not by PowerShell's error records (a script that throws, such
# as build-apk.ps1 without an NDK, fails its step and the others still run).
function Step([string]$name, [scriptblock]$run) {
    $ErrorActionPreference = "Continue"
    $global:LASTEXITCODE = 0
    try {
        & $run | Out-Host
        $verdict = if ($LASTEXITCODE -eq 0) { "ok" } else { "FAILED (exit $LASTEXITCODE)" }
    } catch { $verdict = "FAILED: $($_.Exception.Message)" }
    $script:results += "{0,-28} {1}" -f $name, $verdict
}

Push-Location $repo
try {
    if ($Only -contains "windows") {
        Step "windows: tests" { cargo test -p inkpdf --lib -p inkpdf-api -p inkpdf-ext 2>&1 | Select-String "test result|FAILED|panicked|^error" }
        Step "windows: release build" { cargo build --release -p inkpdf -p inkpdf-thumb 2>&1 | Select-Object -Last 1 }
    }
    if ($Only -contains "android") {
        # Phones and tablets (arm64) and the emulator on this PC (x86_64), in one APK.
        Step "android: APK" { & (Join-Path $repo "tools\android\build-apk.ps1") -Abis arm64-v8a, x86_64 2>&1 | ForEach-Object { "$_" } | Select-Object -Last 3 }
    }
    if ($Only -contains "linux") {
        $out = Join-Path $dist "linux"
        New-Item -ItemType Directory -Force $out | Out-Null
        Step "linux (wsl): tests, package" { wsl -d $Distro -- bash (Wsl-Path (Join-Path $repo "tools\linux\build-all.sh")) linux (Wsl-Path $out) 2>&1 | ForEach-Object { "$_" } | Select-String "^==|test result|FAILED|panicked|^error|built |\.tar\.gz" }
    }
} finally { Pop-Location }
""
$results
"in {0:N0} s" -f $clock.Elapsed.TotalSeconds
