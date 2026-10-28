# Rebuilds icons/svg/*.svg from the TikZ sources in this folder. Needs MiKTeX (latex, dvisvgm).
# The app embeds the SVGs and flattens their paths at start-up, so run this only after changing
# an icon: `powershell icons/build.ps1` from apps/inkpdf.
# Native tools print warnings on stderr (MiKTeX nags about updates); judge them by exit code.
$ErrorActionPreference = "Continue"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$miktex = "C:\Program Files\MiKTeX\miktex\bin\x64"
if (Test-Path $miktex) { $env:PATH += ";$miktex" }
$work = Join-Path ([IO.Path]::GetTempPath()) "inkpdf-icons"
New-Item -ItemType Directory -Force $work, (Join-Path $here "svg") | Out-Null
Copy-Item (Join-Path $here "style.tex") $work -Force
foreach ($tex in Get-ChildItem $here -Filter *.tex | Where-Object Name -ne "style.tex") {
    Copy-Item $tex.FullName $work -Force
    Push-Location $work
    try {
        & latex -interaction=nonstopmode -halt-on-error $tex.Name | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "latex failed on $($tex.Name); see $work\$($tex.BaseName).log" }
        $svg = Join-Path $here "svg\$($tex.BaseName).svg"
        & dvisvgm --no-fonts --precision=4 --bbox=papersize "$($tex.BaseName).dvi" -o $svg | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "dvisvgm failed on $($tex.Name)" }
        "built $($tex.BaseName).svg"
    } finally { Pop-Location }
}
