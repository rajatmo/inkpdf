# Build a signed APK of InkPDF for Android on Windows, with Android Studio's own SDK, NDK and Java (the Windows twin of build-apk.sh,
# which CI and Linux use; keep the two in step). A test build: signed with the debug key in this folder.
#   .\tools\android\build-apk.ps1 [-Build release|debug] [-Out file.apk] [-Abis arm64-v8a,x86_64]
# arm64-v8a is for phones and tablets, x86_64 for the emulator on a PC. Needs: the NDK (Android Studio > SDK Manager > SDK Tools >
# "NDK (Side by side)", r27 or newer), the Rust targets (`rustup target add aarch64-linux-android x86_64-linux-android`), the SDK's
# build-tools and a platform (Android Studio installs them). Finds them in %LOCALAPPDATA%\Android\Sdk and Android Studio's own Java
# unless ANDROID_HOME, ANDROID_NDK_HOME, ANDROID_BUILD_TOOLS, ANDROID_JAR or JAVA_HOME say otherwise.
param([string]$Build = "release", [string]$Out = "", [string[]]$Abis = @("arm64-v8a"))
$ErrorActionPreference = "Stop"
$here = $PSScriptRoot
$root = (Resolve-Path (Join-Path $here "..\..")).Path
$api = 26   # the oldest Android the build runs on (8.0)

# The newest folder in `dir` whose name sorts last as a version.
function Newest([string]$dir) {
    Get-ChildItem $dir -Directory -ErrorAction SilentlyContinue | Sort-Object { [version](($_.Name -replace '[^0-9.]', '') -replace '\.$', '' -replace '^$', '0') } | Select-Object -Last 1 -ExpandProperty FullName
}
$sdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { Join-Path $env:LOCALAPPDATA "Android\Sdk" }
$ndk = if ($env:ANDROID_NDK_HOME) { $env:ANDROID_NDK_HOME } else { Newest (Join-Path $sdk "ndk") }
$tools = if ($env:ANDROID_BUILD_TOOLS) { $env:ANDROID_BUILD_TOOLS } else { Newest (Join-Path $sdk "build-tools") }
$jar = if ($env:ANDROID_JAR) { $env:ANDROID_JAR } else { Join-Path (Newest (Join-Path $sdk "platforms")) "android.jar" }
if (-not $env:JAVA_HOME) { $env:JAVA_HOME = "C:\Program Files\Android\Android Studio\jbr" }
$env:PATH = "$env:JAVA_HOME\bin;$env:PATH"
if (-not $ndk -or -not (Test-Path $ndk)) { throw "no Android NDK in $sdk\ndk: install it in Android Studio (SDK Manager > SDK Tools > NDK (Side by side)) or set ANDROID_NDK_HOME" }
if (-not $tools -or -not (Test-Path "$tools\aapt2.exe")) { throw "no SDK build-tools in $sdk\build-tools (set ANDROID_BUILD_TOOLS)" }
if (-not (Test-Path $jar)) { throw "no android.jar in $sdk\platforms (set ANDROID_JAR)" }
$llvm = Join-Path $ndk "toolchains\llvm\prebuilt\windows-x86_64\bin"
$version = (Select-String -Path (Join-Path $root "apps\inkpdf\Cargo.toml") -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
if (-not $Out) { $Out = Join-Path (Split-Path -Parent $root) "inkpdf-dist\InkPDF-$version-android.apk" }
New-Item -ItemType Directory -Force (Split-Path -Parent $Out) | Out-Null
$work = Join-Path ([IO.Path]::GetTempPath()) ("inkpdf-apk-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force $work | Out-Null

# cargo, javac and the SDK's tools write progress to stderr: judge them by exit code.
function Run([string]$what, [scriptblock]$cmd) {
    $ErrorActionPreference = "Continue"
    & $cmd 2>&1 | ForEach-Object { "$_" } | Where-Object { $_ -match "error|warning: unused|Finished|Verified|Signer" }
    if ($LASTEXITCODE -ne 0) { throw "$what failed (exit $LASTEXITCODE)" }
}

try {
    $flags = @(); if ($Build -eq "release") { $flags += "--release"; $env:CARGO_PROFILE_RELEASE_STRIP = "symbols" }
    # Android 15 devices may use 16 KB pages: the library's segments are aligned for them.
    $env:RUSTFLAGS = "$env:RUSTFLAGS -C link-arg=-Wl,-z,max-page-size=16384".Trim()
    $targetDir = (cargo metadata --format-version 1 --no-deps --manifest-path (Join-Path $root "Cargo.toml") | ConvertFrom-Json).target_directory
    foreach ($abi in $Abis) {
        $target = @{ "arm64-v8a" = "aarch64-linux-android"; "x86_64" = "x86_64-linux-android" }[$abi]
        if (-not $target) { throw "unknown ABI $abi (arm64-v8a or x86_64)" }
        $var = $target -replace '-', '_'; $up = $var.ToUpper()
        # The NDK's compiler is the linker, and the C compiler for the few crates that have C in them.
        Set-Item "env:CC_$var" "$llvm\$target$api-clang.cmd"; Set-Item "env:CXX_$var" "$llvm\$target$api-clang++.cmd"; Set-Item "env:AR_$var" "$llvm\llvm-ar.exe"
        Set-Item "env:CARGO_TARGET_${up}_LINKER" "$llvm\$target$api-clang.cmd"
        Run "cargo build ($abi)" { cargo build @flags -p inkpdf-android --target $target --manifest-path (Join-Path $root "Cargo.toml") }
        $so = Join-Path $targetDir "$target\$Build\libinkpdf_android.so"
        if (-not (Test-Path $so)) { throw "no $so" }
        New-Item -ItemType Directory -Force "$work\lib\$abi" | Out-Null
        Copy-Item $so "$work\lib\$abi\"
    }

    # The Java side (java/app/inkpdf/test/Host.java: the file picker, the clipboard, links): compiled for Java 8 and turned into classes.dex.
    New-Item -ItemType Directory -Force "$work\classes" | Out-Null
    Run "javac" { javac --release 8 -Xlint:-options -cp $jar -d "$work\classes" "$here\java\app\inkpdf\test\Host.java" }
    $classes = Get-ChildItem "$work\classes" -Recurse -Filter *.class | ForEach-Object { $_.FullName }
    Run "d8" { & "$tools\d8.bat" --min-api $api --lib $jar --output $work @classes }
    if (-not (Test-Path "$work\classes.dex")) { throw "no classes.dex" }

    # The package: manifest and resource table by aapt2, the library and the dex added to it, aligned, signed.
    Run "aapt2 compile" { & "$tools\aapt2.exe" compile --dir "$here\res" -o "$work\res.zip" }
    Run "aapt2 link" { & "$tools\aapt2.exe" link --manifest "$here\AndroidManifest.xml" -I $jar -o "$work\base.apk" --min-sdk-version $api --target-sdk-version 34 --version-code 1 --version-name "$version-$Build" "$work\res.zip" }
    Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::Open("$work\base.apk", [IO.Compression.ZipArchiveMode]::Update)
    try {
        foreach ($f in Get-ChildItem "$work\lib" -Recurse -File) {
            $entry = "lib/" + $f.Directory.Name + "/" + $f.Name
            [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, $f.FullName, $entry)
        }
        [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, "$work\classes.dex", "classes.dex")
    } finally { $zip.Dispose() }
    Run "zipalign" { & "$tools\zipalign.exe" -f -p 4 "$work\base.apk" "$work\aligned.apk" }
    Run "apksigner sign" { & "$tools\apksigner.bat" sign --ks "$here\debug.keystore" --ks-pass pass:android --key-pass pass:android --ks-key-alias androiddebugkey --out $Out "$work\aligned.apk" }
    Run "apksigner verify" { & "$tools\apksigner.bat" verify --print-certs $Out }
    "built $Out ({0:N1} MB)" -f ((Get-Item $Out).Length / 1MB)
} finally {
    Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
}
