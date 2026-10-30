# Makes docs/demo.gif: drives the real app through what it can do (the same synthetic stylus, finger, mouse and keyboard as tools/e2e), takes a
# picture of the window after each step, puts a caption on it and joins them into a GIF (with ImageMagick in WSL: `convert`).
# It takes over the mouse and keyboard for a minute or two: do not touch the machine while it runs. Needs the release build
# (`cargo build --release -p inkpdf`) and MiKTeX-free tools only.
#   powershell tools\demo\record.ps1
. "$PSScriptRoot\..\e2e\lib.ps1"
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices;
public static class Win { [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hgt, bool repaint); }
"@

$frames = Join-Path $script:sp "demo"
Remove-Item $frames -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $frames | Out-Null
$script:list = @()
$script:count = 0

# A picture of the window with `$caption` on it, kept for `$ms` milliseconds in the GIF, scaled to `$width` pixels across.
function Frame([string]$caption, [int]$ms = 120, [int]$width = 960) {
    Start-Sleep -Milliseconds 250
    $png = Shot "demo-raw"
    $src = [System.Drawing.Image]::FromFile($png)
    $height = [int]($src.Height * $width / $src.Width)
    $bmp = New-Object System.Drawing.Bitmap $width, $height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $g.DrawImage($src, 0, 0, $width, $height)
    if ($caption) {
        $font = New-Object System.Drawing.Font "Segoe UI", ([single]($width / 48)), ([System.Drawing.FontStyle]::Bold)
        $size = $g.MeasureString($caption, $font)
        $x = ($width - $size.Width) / 2; $y = $height - $size.Height - 14
        $g.FillRectangle((New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb(215, 28, 30, 36))), $x - 14, $y - 4, $size.Width + 28, $size.Height + 8)
        $g.DrawString($caption, $font, [System.Drawing.Brushes]::White, $x, $y)
    }
    $script:count++
    $file = Join-Path $frames ("{0:D3}.png" -f $script:count)
    $bmp.Save($file, [System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose(); $src.Dispose()
    $script:list += @{ file = $file; ms = $ms }
}

# A window of this size (so that the interface is not tiny in a GIF a thousand pixels across), and what the bot and the clicks then use.
function Size-Window([int]$w, [int]$h) {
    [void][Win]::MoveWindow($script:app.MainWindowHandle, 60, 40, $w, $h, $true); Start-Sleep -Milliseconds 800
    if ((& $script:bot $script:name size) -match "client (\d+)x(\d+) at \((-?\d+), (-?\d+)\)") { $script:cw = [int]$Matches[1]; $script:ch = [int]$Matches[2]; $script:ox = [int]$Matches[3]; $script:oy = [int]$Matches[4] }
}

# ---- 1. Ink, colours, shapes, text: a blank page, pen first.
Launch "tool pen`nslot 0`nshape-slot 3`ntext-slot 2`ntext-size 36.00`n" "" @("--mode", "desktop")
Size-Window 1240 860
Frame "A blank page, and your pen" 900
B stroke 0.2 0.3 0.45 0.34 0.03 2 40; Frame "Ink that keeps up with the pen" 350
Bar swatch1; B stroke 0.25 0.42 0.5 0.38 0.03 2 40; Frame "" 250
Bar swatch2; B stroke 0.3 0.5 0.55 0.54 0.03 2 40; Frame "Any colour, any thickness" 700
Press geometry; B line 0.6 0.3 0.8 0.5 20; Frame "Shapes you can edit by their nodes" 900
Press text; Click 0.55 0.7; Type-Text "Typed text"; Keys "{ESC}"; Frame "Typed text, saved as real PDF annotations" 1000
Press lasso
foreach ($try in @(@(0.33, 0.31), @(0.3, 0.33), @(0.36, 0.34), @(0.27, 0.3))) { Click $try[0] $try[1]; if ((Accent) -gt 80) { break } }
Frame "The lasso picks things up" 800
Keys "{DEL}"; Press eraser; Mouse-Drag 0.28 0.5 0.55 0.55; Frame "The eraser takes away what it passes over" 900

# ---- 2. Reading: select, mark and search the text of a PDF.
Launch "tool select`n" "pages3" @("--mode", "desktop")
Size-Window 1240 860
Frame "Open any PDF" 900
Mouse-Drag 0.2 0.30 0.6 0.30; Frame "Select its text and copy it" 800
Bar highlight; Frame "Highlight, underline, strike out" 900
Keys "^f"; Type-Text "latency"; Start-Sleep -Milliseconds 600; Frame "Search finds every place" 1100
Keys "{ESC}"
Press theme; Frame "Light, dark, or the system's look, and dark pages" 1300
Press theme; Press theme; Click 0.5 0.7
Press fold-doc; Press pages; Start-Sleep -Milliseconds 500; Frame "Every page at a glance: reorder, duplicate, delete, flag" 1400
Keys "{ESC}"

# ---- 2b. A form: boxes are lit, then filled with clicks and typing.
Launch "tool select`n" "form" @("--mode", "desktop")
Size-Window 1240 860
Frame "Fill in a PDF form" 900
Click-Spot (Spot fields agree); Click-Spot (Spot fields "size#1"); Click-Spot (Spot fields colour)
Click-Spot (Spot fields name); Type-Text "Ada Lovelace"; Start-Sleep -Milliseconds 300
Frame "Text, boxes, buttons, choices" 1600
Keys "{ENTER}"

# ---- 3. A narrow screen: the toolbar folds into round buttons.
Launch "tool pen`n" "" @("--mode", "tablet")
Size-Window 470 940
Frame "On a phone the toolbar folds away" 1100 480
try { Click-Spot (Spot buttons fold-doc); Frame "Tap a round button to unfold it" 1200 480 } catch { }
Stop-App

# ---- Join them.
$wsl = "/mnt/" + $frames.Substring(0, 1).ToLower() + ($frames.Substring(2) -replace '\\', '/')
$args_ = ($script:list | ForEach-Object { "-delay {0} '{1}'" -f [int]($_.ms / 10), (Split-Path $_.file -Leaf) }) -join " "
$out = Join-Path $script:repo "docs\demo.gif"
$wslOut = "/mnt/" + $out.Substring(0, 1).ToLower() + ($out.Substring(2) -replace '\\', '/')
$cmd = "cd '$wsl' && convert $args_ -loop 0 -layers Optimize -colors 128 '$wslOut' && ls -la '$wslOut'"
$sh = Join-Path $frames "join.sh"
[IO.File]::WriteAllText($sh, $cmd + "`n", (New-Object System.Text.UTF8Encoding $false))
wsl -- bash ("/mnt/" + $sh.Substring(0, 1).ToLower() + ($sh.Substring(2) -replace '\\', '/'))
"$($script:list.Count) frames -> $out"
