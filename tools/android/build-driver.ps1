# Build the input driver of the real-input checks (driver/, a small instrumentation: not InkPDF, never shipped) with Android Studio's own
# SDK and Java, signed with the debug key in this folder, to inkpdf-dist\InkPDF-input-driver.apk beside the repository.
#   .\tools\android\build-driver.ps1 [-Out file.apk]
param([string]$Out = "")
$ErrorActionPreference = "Stop"
$here = $PSScriptRoot
$root = (Resolve-Path (Join-Path $here "..\..")).Path
$api = 26

function Newest([string]$dir) {
    Get-ChildItem $dir -Directory -ErrorAction SilentlyContinue | Sort-Object { [version](($_.Name -replace '[^0-9.]', '') -replace '\.$', '' -replace '^$', '0') } | Select-Object -Last 1 -ExpandProperty FullName
}
$sdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { Join-Path $env:LOCALAPPDATA "Android\Sdk" }
$tools = if ($env:ANDROID_BUILD_TOOLS) { $env:ANDROID_BUILD_TOOLS } else { Newest (Join-Path $sdk "build-tools") }
$jar = if ($env:ANDROID_JAR) { $env:ANDROID_JAR } else { Join-Path (Newest (Join-Path $sdk "platforms")) "android.jar" }
if (-not $env:JAVA_HOME) { $env:JAVA_HOME = "C:\Program Files\Android\Android Studio\jbr" }
$env:PATH = "$env:JAVA_HOME\bin;$env:PATH"
if (-not $Out) { $Out = Join-Path (Split-Path -Parent $root) "inkpdf-dist\InkPDF-input-driver.apk" }
New-Item -ItemType Directory -Force (Split-Path -Parent $Out) | Out-Null
$work = Join-Path ([IO.Path]::GetTempPath()) ("inkpdf-driver-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force "$work\classes" | Out-Null

function Run([string]$what, [scriptblock]$cmd) {
    $ErrorActionPreference = "Continue"
    & $cmd 2>&1 | ForEach-Object { "$_" } | Where-Object { $_ -match "error|Verified|Signer" }
    if ($LASTEXITCODE -ne 0) { throw "$what failed (exit $LASTEXITCODE)" }
}

try {
    Run "javac" { javac --release 8 -Xlint:-options -cp $jar -d "$work\classes" "$here\driver\app\inkpdf\driver\Drive.java" }
    $classes = Get-ChildItem "$work\classes" -Recurse -Filter *.class | ForEach-Object { $_.FullName }
    Run "d8" { & "$tools\d8.bat" --min-api $api --lib $jar --output $work @classes }
    Run "aapt2 link" { & "$tools\aapt2.exe" link --manifest "$here\driver\AndroidManifest.xml" -I $jar -o "$work\base.apk" --min-sdk-version $api --target-sdk-version 34 --version-code 1 --version-name 1 }
    Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::Open("$work\base.apk", [IO.Compression.ZipArchiveMode]::Update)
    try { [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, "$work\classes.dex", "classes.dex") } finally { $zip.Dispose() }
    Run "zipalign" { & "$tools\zipalign.exe" -f -p 4 "$work\base.apk" "$work\aligned.apk" }
    Run "apksigner sign" { & "$tools\apksigner.bat" sign --ks "$here\debug.keystore" --ks-pass pass:android --key-pass pass:android --ks-key-alias androiddebugkey --out $Out "$work\aligned.apk" }
    "built $Out ({0:N0} KB)" -f ((Get-Item $Out).Length / 1KB)
} finally {
    Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
}
