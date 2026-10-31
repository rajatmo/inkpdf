# Runs the real-input checks, the same ones on every platform (scenarios.py), against the built app and says what passed.
# They drive the REAL app with the real devices the platform has: on Windows they take over the mouse and keyboard (and a synthetic
# stylus and finger) for about a minute and need an unlocked desktop, so do not touch the machine while they run; on Linux they run
# on a virtual display in WSL; on Android they run on the emulators (and install the APK from inkpdf-dist first).
#   .\tools\e2e\run.ps1                              Windows, Linux (WSL) and both Android emulators
#   .\tools\e2e\run.ps1 -Platform windows            one platform (windows, linux, android)
#   .\tools\e2e\run.ps1 -Only pages                  one scenario (devices, pages, reader, forms)
#   .\tools\e2e\run.ps1 -Platform android -Avd Medium_Tablet_API_37.0
# Build first: `cargo build --release -p inkpdf --bins` (Windows; the Linux build is made here, in WSL), and
# `.\tools\android\build-apk.ps1 -Abis arm64-v8a,x86_64` (Android).
param(
    [ValidateSet("windows", "linux", "android")][string[]]$Platform = @("windows", "linux", "android"),
    [string[]]$Only,
    [string[]]$Avd = @("Medium_Phone_API_37.0", "Medium_Tablet_API_37.0")
)
# (Native tools write to stderr for what is not an error, such as adb having no emulator to stop: failures are thrown here.)
$ErrorActionPreference = "Continue"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$py = Join-Path $PSScriptRoot "run.py"
$onlyArgs = @(if ($Only) { "--only"; $Only })
$sdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { Join-Path $env:LOCALAPPDATA "Android\Sdk" }
$adb = Join-Path $sdk "platform-tools\adb.exe"
$failed = 0
$summary = @()

function Run-Windows {
    & python $py windows @onlyArgs | Out-Host
    return $LASTEXITCODE
}

function Run-Linux {
    $wsl = (& wsl -d Ubuntu -- wslpath -a ($root -replace "\\", "/")).Trim()
    & wsl -d Ubuntu -- bash -lc "cd '$wsl' && bash tools/e2e/linux.sh $($onlyArgs -join ' ')" | Out-Host
    return $LASTEXITCODE
}

function Wait-Boot {
    & $adb wait-for-device
    foreach ($i in 1..120) {
        if (((& $adb shell getprop sys.boot_completed 2>$null) -join "").Trim() -eq "1") { return }
        Start-Sleep -Seconds 3
    }
    throw "the emulator did not finish starting"
}

function Run-Android {
    $version = (Select-String -Path (Join-Path $root "apps\inkpdf\Cargo.toml") -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
    $apk = Join-Path (Split-Path -Parent $root) "inkpdf-dist\InkPDF-$version-android.apk"
    if (-not (Test-Path $apk)) { throw "no APK at ${apk}: build it with tools\android\build-apk.ps1 -Abis arm64-v8a,x86_64" }
    # (The input driver app, a pen and fingers for the checks: built once.)
    if (-not (Test-Path (Join-Path (Split-Path -Parent $root) "inkpdf-dist\InkPDF-input-driver.apk"))) { & (Join-Path $root "tools\android\build-driver.ps1") | Out-Host }
    $code = 0
    foreach ($name in $Avd) {
        & $adb emu kill 2>$null | Out-Null
        Start-Sleep -Seconds 6
        Start-Process (Join-Path $sdk "emulator\emulator.exe") -ArgumentList "-avd", $name, "-no-snapshot-save" -WindowStyle Minimized
        Start-Sleep -Seconds 8
        Wait-Boot
        & $adb install -r $apk | Out-Host
        Write-Host "---- android: $name"
        & python $py android @onlyArgs | Out-Host
        if ($LASTEXITCODE -ne 0) { $code = 1 }
        & $adb emu kill 2>$null | Out-Null
    }
    return $code
}

foreach ($p in $Platform) {
    Write-Host "======== $p"
    $code = switch ($p) { "windows" { Run-Windows } "linux" { Run-Linux } "android" { Run-Android } }
    $summary += "{0,-8} {1}" -f $p, $(if ($code -eq 0) { "all passed" } else { "FAILED" })
    if ($code -ne 0) { $failed = 1 }
}
"`n" + ($summary -join "`n")
exit $failed
