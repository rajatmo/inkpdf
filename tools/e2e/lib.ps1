# The few helpers the real-input checks need: start the built app, give it a stylus and finger (inkpdf-bot), the real mouse
# and keyboard, take a screenshot and count pixels of a colour. Buttons are pressed by NAME: the app is started with
# `--layout FILE` and keeps that file saying where every button, bar action, overview page and contents entry is (see
# apps/inkpdf/src/app/layout.rs), so no position is remembered here and any window size works.
# APPDATA always points into a scratch folder, never the user's settings. Nothing is written inside the repository: scratch
# files go to `inkpdf-e2e` beside it (or INKPDF_E2E_DIR), and the release build under test is read from `inkpdf-build` beside it
# (CARGO_TARGET_DIR if you build elsewhere): `cargo build --release -p inkpdf`.
$script:repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$script:workspace = Split-Path -Parent $script:repo
$script:sp = if ($env:INKPDF_E2E_DIR) { $env:INKPDF_E2E_DIR } else { Join-Path $script:workspace "inkpdf-e2e" }
New-Item -ItemType Directory -Force $script:sp | Out-Null
$script:target = Join-Path $(if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $script:workspace "inkpdf-build" }) "release"
$script:bot = "$script:target\inkpdf-bot.exe"
$script:layoutFile = Join-Path $script:sp "layout.json"

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @"
using System; using System.Drawing; using System.Drawing.Imaging; using System.Runtime.InteropServices;
public static class W {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, UIntPtr e);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr h, uint msg, IntPtr w, string l, uint flags, uint timeout, out IntPtr result);
    // Pixels within `tol` of (r,g,b) in a rectangle of a PNG, in the PNG's own pixels.
    public static int Count(string path, int r, int g, int b, int tol, double x0, double y0, double x1, double y1) {
        using (var bmp = new Bitmap(path)) {
            int ax = Math.Max(0, (int)x0), ay = Math.Max(0, (int)y0), bx = Math.Min(bmp.Width, (int)x1), by = Math.Min(bmp.Height, (int)y1);
            var d = bmp.LockBits(new Rectangle(0, 0, bmp.Width, bmp.Height), ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
            byte[] buf = new byte[Math.Abs(d.Stride) * bmp.Height];
            Marshal.Copy(d.Scan0, buf, 0, buf.Length);
            bmp.UnlockBits(d);
            int n = 0;
            for (int y = ay; y < by; y++) for (int x = ax; x < bx; x++) {
                int i = y * d.Stride + x * 4;
                if (Math.Abs(buf[i + 2] - r) <= tol && Math.Abs(buf[i + 1] - g) <= tol && Math.Abs(buf[i] - b) <= tol) n++;
            }
            return n;
        }
    }
}
"@
[void][W]::SetProcessDPIAware()

# ---- The app
# Start the app on a blank document, or on a copy of the PDF `$Pdf` ("pages3": three pages from the build's own generator,
# "outline12": twelve pages with a table of contents, or a name in the scratch folder), with the settings `$Settings` (the light
# look is always added, so the colours counted are the light ones) and extra arguments.
function Launch([string]$Settings = "", [string]$Pdf = "", [string[]]$AppArgs = @()) {
    Get-Process inkpdf -ErrorAction SilentlyContinue | Stop-Process -Force
    $env:APPDATA = Join-Path $script:sp "appdata"
    Remove-Item "$env:APPDATA\InkPDF" -Recurse -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force "$env:APPDATA\InkPDF" | Out-Null
    Set-Content "$env:APPDATA\InkPDF\settings.txt" ($Settings + "appearance light`n") -Encoding ascii
    $script:file = $null
    if ($Pdf) {
        $script:file = Join-Path $script:sp "work-$Pdf.pdf"
        if ($Pdf -match "^pages(\d+)$") { & "$script:target\inkpdf-gen.exe" $script:file $Matches[1] | Out-Null }
        elseif ($Pdf -match "^outline(\d+)$") { & "$script:target\inkpdf-gen.exe" $script:file $Matches[1] --outline | Out-Null }
        else { Copy-Item (Join-Path $script:sp $Pdf) $script:file -Force }
    }
    $script:appArgs = @($AppArgs)
    Start-App
}
function Start-App {
    Remove-Item $script:layoutFile -ErrorAction SilentlyContinue
    $a = @(if ($script:file) { "`"$script:file`"" }) + @("--topmost", "--layout", "`"$script:layoutFile`"") + @($script:appArgs)
    $script:app = Start-Process "$script:target\inkpdf.exe" -ArgumentList $a -PassThru -RedirectStandardOutput (Join-Path $script:sp "app.out.txt") -RedirectStandardError (Join-Path $script:sp "app.err.txt")
    # Ready when the window has a title and the app has said where its buttons are (not a fixed wait).
    foreach ($i in 1..80) { Start-Sleep -Milliseconds 150; $script:app.Refresh(); if ($script:app.MainWindowTitle -and (Test-Path $script:layoutFile)) { break } }
    [void][W]::SetForegroundWindow($script:app.MainWindowHandle)
    Start-Sleep -Milliseconds 300
    $script:name = if ($script:file) { [IO.Path]::GetFileName($script:file) } else { "Untitled" }
    if ((& $script:bot $script:name size) -match "client (\d+)x(\d+) at \((-?\d+), (-?\d+)\)") { $script:cw = [int]$Matches[1]; $script:ch = [int]$Matches[2]; $script:ox = [int]$Matches[3]; $script:oy = [int]$Matches[4] }
}
function Reopen { Stop-App; Start-App }
function Title { $script:app.Refresh(); $script:app.MainWindowTitle }
function Alive { $script:app.Refresh(); -not $script:app.HasExited }
function Stop-App { if ($script:app -and -not $script:app.HasExited) { $script:app.Kill(); $script:app.WaitForExit(3000) | Out-Null } }
function Errs { @(Get-Content (Join-Path $script:sp "app.err.txt") | Where-Object { $_ -match "panick|error|fail" } | Select-Object -First 5) }
function Pages { if ((Title) -match "page (\d+)/(\d+)") { @([int]$Matches[1], [int]$Matches[2]) } else { @(0, 0) } }
function Dirty { (Title).Contains([string][char]0x25CF) }
function Healthy { (Alive) -and ((Errs).Count -eq 0) }

# ---- Where things are, from the app: `Spot buttons undo`, `Spot bar delete`, `Spot overview/pages 3`, `Spot overview/toc "Chapter 2"`.
# Window pixels; waits up to a second for something that is about to appear (the overview opening, a tool's bar).
function Spot([string]$group, [string]$name) {
    foreach ($try in 1..20) {
        $o = $null
        try { $o = Get-Content $script:layoutFile -Raw -ErrorAction Stop | ConvertFrom-Json } catch {}
        foreach ($g in $group.Split('/')) { if ($o) { $o = $o.$g } }
        if ($o -and $o.PSObject.Properties[$name]) { return ,@($o.PSObject.Properties[$name].Value) }
        Start-Sleep -Milliseconds 50
    }
    throw "the app's layout has no '$name' in $group"
}

# ---- Input. Page positions are fractions of the window; controls are named.
function B { & $script:bot $script:name @args | Out-Null; Start-Sleep -Milliseconds 120 }   # stylus and finger
function Mouse-Px([int]$x0, [int]$y0, [int]$x1, [int]$y1, [string]$button = "left") {
    $down = @{ left = 2; right = 8; middle = 32 }[$button]; $up = @{ left = 4; right = 16; middle = 64 }[$button]
    [void][W]::SetCursorPos($script:ox + $x0, $script:oy + $y0); Start-Sleep -Milliseconds 80
    [W]::mouse_event($down, 0, 0, 0, [UIntPtr]::Zero)
    foreach ($i in 1..12) { [void][W]::SetCursorPos([int]($script:ox + $x0 + ($x1 - $x0) * $i / 12), [int]($script:oy + $y0 + ($y1 - $y0) * $i / 12)); Start-Sleep -Milliseconds 10 }
    [W]::mouse_event($up, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 150
}
function Mouse-Drag($fx0, $fy0, $fx1, $fy1) { Mouse-Px ($fx0 * $script:cw) ($fy0 * $script:ch) ($fx1 * $script:cw) ($fy1 * $script:ch) }
function Click($fx, $fy) { Mouse-Drag $fx $fy $fx $fy }
function Click-Spot($p) { Mouse-Px $p[0] $p[1] $p[0] $p[1] }
function Tap-Spot($p) { B tap ($p[0] / $script:cw) ($p[1] / $script:ch) }
function Type-Text([string]$s) {
    [System.Windows.Forms.SendKeys]::SendWait(($s.ToCharArray() | ForEach-Object { if ('+^%~(){}[]'.Contains($_)) { "{$_}" } else { "$_" } }) -join "")
    Start-Sleep -Milliseconds 150
}
function Keys([string]$k) { [System.Windows.Forms.SendKeys]::SendWait($k); Start-Sleep -Milliseconds 150 }
# Toolbar buttons and tools by name (undo, redo, save, del, plus, pages, search, theme, mode, full; select, pen, geometry, text,
# lasso, eraser, laser), with the mouse, the stylus tip or a finger; the bar under the tools likewise (delete, copy, duplicate,
# paste, swatch0..swatch4, style, highlight, …).
function Press($name) { Click-Spot (Spot buttons $name) }
function Pen-Press($name) { Tap-Spot (Spot buttons $name) }
function Finger-Press($name) { $p = Spot buttons $name; $f = @(($p[0] / $script:cw), ($p[1] / $script:ch)); B finger down 1 $f[0] $f[1]; B finger up 1 $f[0] $f[1] }
function Bar($name) { Click-Spot (Spot bar $name) }
function Pen-Bar($name) { Tap-Spot (Spot bar $name) }

# ---- Looking: pixels of a colour, from a fresh screenshot, in window pixels (the page area, under the toolbar, by default).
function Shot([string]$n) {
    $script:app.Refresh(); $h = $script:app.MainWindowHandle
    $c = New-Object W+RECT; [void][W]::GetClientRect($h, [ref]$c)
    $pt = New-Object W+POINT; [void][W]::ClientToScreen($h, [ref]$pt)
    $bmp = New-Object System.Drawing.Bitmap ($c.Right - $c.Left), ($c.Bottom - $c.Top)
    $g = [System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($pt.X, $pt.Y, 0, 0, $bmp.Size)
    $path = Join-Path $script:sp "$n.png"; $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose(); $path
}
function Count($r, $g, $b, $x0 = -1, $y0 = -1, $x1 = -1, $y1 = -1) {
    if ($x0 -lt 0) { $x0 = 0.15 * $script:cw; $y0 = 0.14 * $script:ch; $x1 = 0.95 * $script:cw; $y1 = 0.93 * $script:ch }
    [W]::Count((Shot "look"), $r, $g, $b, 45, $x0, $y0, $x1, $y1)
}
# The same colour within `half` pixels of a spot (a button, a page's picture).
function Count-Near($rgb, $p, [int]$hw = 18, [int]$hh = -1) { if ($hh -lt 0) { $hh = $hw }; Count $rgb[0] $rgb[1] $rgb[2] ($p[0] - $hw) ($p[1] - $hh) ($p[0] + $hw) ($p[1] + $hh) }
$script:black = @(28, 30, 36); $script:red = @(224, 52, 52); $script:accent = @(64, 132, 255)
function Black { Count 28 30 36 }       # the pen's colour slot 0
function Red { Count 224 52 52 }        # colour slot 2
function Accent { Count 64 132 255 }    # the interface's blue: a selection's frame, a shape's anchors

# ---- Windows telling the app about its settings (posture and others), as the system does.
function Tell-Setting([string]$name) {
    $r = [IntPtr]::Zero; [void][W]::SendMessageTimeout($script:app.MainWindowHandle, 0x1A, [IntPtr]::Zero, $name, 2, 3000, [ref]$r); Start-Sleep -Milliseconds 500
}

# ---- Reporting
function Check($cond, $m) { if ($cond) { Write-Host "ok:   $m" } else { Write-Host "FAIL: $m" -ForegroundColor Red } }
