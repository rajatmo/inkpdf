# Builds the app's icons from its TikZ drawings. Needs MiKTeX (pdflatex, pdftocairo).
#   app-icon.tex          -> app.ico (16-256 px) and app-64.png, app-256.png here, for Windows;
#   app-icon-android.tex  -> the Android launcher icons in tools/android/res (the tile for launchers before Android 8, the pen alone for
#                            the adaptive icon and the themed one).
# Both draw the pen of pen-art.tex, so they are the same picture. Run after changing a drawing: `powershell installer/make-icon.ps1` from apps/inkpdf.
$ErrorActionPreference = "Continue"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$miktex = "C:\Program Files\MiKTeX\miktex\bin\x64"
if (Test-Path $miktex) { $env:PATH += ";$miktex" }
$work = Join-Path ([IO.Path]::GetTempPath()) "inkpdf-appicon"
New-Item -ItemType Directory -Force $work | Out-Null
foreach ($name in "app-icon.tex", "app-icon-android.tex", "pen-art.tex") { Copy-Item (Join-Path $here $name) $work -Force }
Push-Location $work
try {
    foreach ($name in "app-icon", "app-icon-android") {
        & pdflatex -interaction=nonstopmode -halt-on-error "$name.tex" | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "pdflatex failed on $name.tex; see $work\$name.log" }
    }
    $sizes = 16, 24, 32, 48, 64, 128, 256
    $pngs = foreach ($s in $sizes) {
        & pdftocairo -png -singlefile -transp -scale-to $s app-icon.pdf "icon-$s" | Out-Null
        [pscustomobject]@{ Size = $s; Bytes = [IO.File]::ReadAllBytes((Join-Path $work "icon-$s.png")) }
    }
    # An .ico is a small directory followed by the images; PNG data is allowed inside (Vista and later).
    $ms = New-Object IO.MemoryStream; $w = New-Object IO.BinaryWriter $ms
    $w.Write([uint16]0); $w.Write([uint16]1); $w.Write([uint16]$pngs.Count)
    $offset = 6 + 16 * $pngs.Count
    foreach ($p in $pngs) {
        $dim = if ($p.Size -ge 256) { 0 } else { $p.Size }
        $w.Write([byte]$dim); $w.Write([byte]$dim); $w.Write([byte]0); $w.Write([byte]0)
        $w.Write([uint16]1); $w.Write([uint16]32); $w.Write([uint32]$p.Bytes.Length); $w.Write([uint32]$offset)
        $offset += $p.Bytes.Length
    }
    foreach ($p in $pngs) { $w.Write($p.Bytes) }
    [IO.File]::WriteAllBytes((Join-Path $here "app.ico"), $ms.ToArray())
    Copy-Item (Join-Path $work "icon-64.png") (Join-Path $here "app-64.png") -Force
    Copy-Item (Join-Path $work "icon-256.png") (Join-Path $here "app-256.png") -Force
    "wrote app.ico ($($pngs.Count) sizes), app-64.png, app-256.png"

    # Android: a launcher icon is 48 dp and an adaptive layer 108 dp, at each density's scale.
    $res = [IO.Path]::GetFullPath((Join-Path $here "..\..\..\tools\android\res"))
    $densities = [ordered]@{ mdpi = 1; hdpi = 1.5; xhdpi = 2; xxhdpi = 3; xxxhdpi = 4 }
    foreach ($density in $densities.Keys) {
        $scale = $densities[$density]
        $dir = Join-Path $res "mipmap-$density"
        New-Item -ItemType Directory -Force $dir | Out-Null
        & pdftocairo -png -singlefile -transp -scale-to ([int][math]::Round(48 * $scale)) app-icon.pdf (Join-Path $dir "ic_launcher") | Out-Null
        & pdftocairo -png -singlefile -transp -scale-to ([int][math]::Round(108 * $scale)) app-icon-android.pdf (Join-Path $dir "ic_launcher_foreground") | Out-Null
    }
    "wrote the Android launcher icons in $res"
} finally { Pop-Location }
