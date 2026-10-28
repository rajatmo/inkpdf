# Builds app.ico (16-256 px) from the TikZ drawing in app-icon.tex. Needs MiKTeX (pdflatex, pdftocairo).
# Run after changing the drawing: `powershell installer/make-icon.ps1` from apps/inkpdf.
$ErrorActionPreference = "Continue"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$miktex = "C:\Program Files\MiKTeX\miktex\bin\x64"
if (Test-Path $miktex) { $env:PATH += ";$miktex" }
$work = Join-Path ([IO.Path]::GetTempPath()) "inkpdf-appicon"
New-Item -ItemType Directory -Force $work | Out-Null
Copy-Item (Join-Path $here "app-icon.tex") $work -Force
Push-Location $work
try {
    & pdflatex -interaction=nonstopmode -halt-on-error app-icon.tex | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "pdflatex failed; see $work\app-icon.log" }
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
} finally { Pop-Location }
