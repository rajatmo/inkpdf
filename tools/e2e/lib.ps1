# Helpers for real-input end-to-end checks of InkPDF: stylus/finger via inkpdf-bot, real keyboard via SendKeys,
# real mouse via mouse_event. Always runs with APPDATA in a scratch folder (never the user's settings).
# Scratch files (screenshots, settings, copies of test PDFs) go here, never next to the sources.
$script:sp = if ($env:INKPDF_E2E_DIR) { $env:INKPDF_E2E_DIR } else { Join-Path $env:TEMP "inkpdf-e2e" }
New-Item -ItemType Directory -Force $script:sp | Out-Null
# The release build under test: `cargo build --release -p inkpdf` (set CARGO_TARGET_DIR if you build elsewhere).
$script:repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$script:target = Join-Path $(if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $script:repo "target" }) "release"

$script:bot = "$script:target\inkpdf-bot.exe"
# The 60-page test PDF some scripts open, made by the build's own generator.
if (-not (Test-Path (Join-Path $script:sp "e2e-pen.pdf"))) { & "$script:target\inkpdf-gen.exe" (Join-Path $script:sp "e2e-pen.pdf") 60 | Out-Null }
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class W {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
    [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, UIntPtr e);
}
"@
[void][W]::SetProcessDPIAware()

function Launch([string]$Settings = "", [string]$Base = "e2e-pen.pdf", [string]$Name = "t1") {
    Get-Process inkpdf -ErrorAction SilentlyContinue | Stop-Process -Force
    $env:APPDATA = Join-Path $script:sp "appdata"
    Remove-Item "$env:APPDATA\InkPDF" -Recurse -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force "$env:APPDATA\InkPDF" | Out-Null
    if ($Settings) { Set-Content "$env:APPDATA\InkPDF\settings.txt" $Settings -Encoding ascii }
    $script:name = $Name
    $script:pdf = Join-Path $script:sp "$Name.pdf"
    if ($Base) { Copy-Item (Join-Path $script:sp $Base) $script:pdf -Force }
    $a = if ($Base) { @("`"$script:pdf`"", "--topmost") } else { @("--topmost") }
    $script:app = Start-Process "$script:target\inkpdf.exe" -ArgumentList $a -PassThru -RedirectStandardOutput (Join-Path $script:sp "app.out.txt") -RedirectStandardError (Join-Path $script:sp "app.err.txt")
    Start-Sleep 3.5
    $script:app.Refresh()
    [void][W]::SetForegroundWindow($script:app.MainWindowHandle)
    Start-Sleep -Milliseconds 400
    $script:title = if ($Base) { $Name } else { "Untitled" }
    $sz = (& $script:bot $script:title size)
    if ($sz -match "client (\d+)x(\d+) at \((-?\d+), (-?\d+)\)") { $script:cw=[int]$Matches[1]; $script:ch=[int]$Matches[2]; $script:ox=[int]$Matches[3]; $script:oy=[int]$Matches[4] }
}
function Title { $script:app.Refresh(); $script:app.MainWindowTitle }
function Alive { $script:app.Refresh(); -not $script:app.HasExited }
function Stop-App { if ($script:app -and -not $script:app.HasExited) { $script:app.Kill(); Start-Sleep -Milliseconds 400 } }
function Errs { Get-Content (Join-Path $script:sp "app.err.txt") | Where-Object { $_ -match "panick|error|fail" } | Select-Object -First 5 }
function B { & $script:bot $script:title @args | Out-Null; Start-Sleep -Milliseconds 250 }
$script:UseMouse = $false
function Tap($x, $y) { if ($script:UseMouse) { Mouse-Click $x $y } else { B tap $x $y } }
function Drag($x0, $y0, $x1, $y1, $steps = 14) { if ($script:UseMouse) { Mouse-Drag $x0 $y0 $x1 $y1 } else { B line $x0 $y0 $x1 $y1 $steps } }
function Esc-Keys([string]$s) { ($s.ToCharArray() | ForEach-Object { if ('+^%~(){}[]'.Contains($_)) { "{$_}" } else { "$_" } }) -join "" }
function Type-Text([string]$s) { [System.Windows.Forms.SendKeys]::SendWait((Esc-Keys $s)); Start-Sleep -Milliseconds 250 }
function Keys([string]$k) { [System.Windows.Forms.SendKeys]::SendWait($k); Start-Sleep -Milliseconds 250 }
function Focus { [void][W]::SetForegroundWindow($script:app.MainWindowHandle); Start-Sleep -Milliseconds 150 }
function Px($fx, $fy) { @([int]($script:ox + $fx * $script:cw), [int]($script:oy + $fy * $script:ch)) }
function Mouse-Click($fx, $fy) { $p = Px $fx $fy; [void][W]::SetCursorPos($p[0], $p[1]); Start-Sleep -Milliseconds 120; [W]::mouse_event(2,0,0,0,[UIntPtr]::Zero); Start-Sleep -Milliseconds 60; [W]::mouse_event(4,0,0,0,[UIntPtr]::Zero); Start-Sleep -Milliseconds 200 }
function Mouse-Drag($fx0, $fy0, $fx1, $fy1) { $p = Px $fx0 $fy0; $q = Px $fx1 $fy1; [void][W]::SetCursorPos($p[0], $p[1]); Start-Sleep -Milliseconds 100; [W]::mouse_event(2,0,0,0,[UIntPtr]::Zero); foreach ($i in 1..15) { [void][W]::SetCursorPos([int]($p[0]+($q[0]-$p[0])*$i/15), [int]($p[1]+($q[1]-$p[1])*$i/15)); Start-Sleep -Milliseconds 12 }; [W]::mouse_event(4,0,0,0,[UIntPtr]::Zero); Start-Sleep -Milliseconds 200 }
function Wheel($notches) { [W]::mouse_event(0x0800,0,0,[BitConverter]::ToUInt32([BitConverter]::GetBytes([int]($notches * 120)),0),[UIntPtr]::Zero); Start-Sleep -Milliseconds 600 }
function Shot([string]$n) {
    $script:app.Refresh(); $h = $script:app.MainWindowHandle
    $c = New-Object W+RECT; [void][W]::GetClientRect($h, [ref]$c)
    $pt = New-Object W+POINT; [void][W]::ClientToScreen($h, [ref]$pt)
    $bmp = New-Object System.Drawing.Bitmap ($c.Right-$c.Left), ($c.Bottom-$c.Top)
    $g = [System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($pt.X, $pt.Y, 0, 0, $bmp.Size)
    $path = Join-Path $script:sp "$n.png"; $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose(); $path
}
# toolbar buttons as window fractions (maximized window, 4K): close, undo, redo, save, plus | pen, shape, text, laser | fullscreen
$script:btn = @{ close=@(0.016,0.031); undo=@(0.0365,0.031); redo=@(0.0545,0.031); save=@(0.0715,0.031); plus=@(0.093,0.031); pen=@(0.4745,0.031); shape=@(0.4915,0.031); text=@(0.5085,0.031); laser=@(0.5255,0.031); full=@(0.9835,0.031) }
function Press($name) { Tap $script:btn[$name][0] $script:btn[$name][1] }
function Fail($m) { Write-Host "FAIL: $m" -ForegroundColor Red }
function Ok($m) { Write-Host "ok:   $m" }
function Check($cond, $m) { if ($cond) { Ok $m } else { Fail $m } }

Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @"
using System; using System.Drawing; using System.Drawing.Imaging; using System.Runtime.InteropServices;
public static class Pix {
    // Pixels within `tol` of (r,g,b) in the rectangle (x0,y0)-(x1,y1) of a PNG, given in display coordinates dispW wide.
    public static int Count(string path, int r, int g, int b, int tol, double x0, double y0, double x1, double y1, double dispW) {
        using (var bmp = new Bitmap(path)) {
            double k = bmp.Width / dispW;
            int ax = Math.Max(0,(int)(x0*k)), ay = Math.Max(0,(int)(y0*k)), bx = Math.Min(bmp.Width,(int)(x1*k)), by = Math.Min(bmp.Height,(int)(y1*k));
            var d = bmp.LockBits(new Rectangle(0,0,bmp.Width,bmp.Height), ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
            byte[] buf = new byte[Math.Abs(d.Stride) * bmp.Height];
            Marshal.Copy(d.Scan0, buf, 0, buf.Length);
            bmp.UnlockBits(d);
            int n = 0;
            for (int y = ay; y < by; y++) for (int x = ax; x < bx; x++) {
                int i = y*d.Stride + x*4;
                if (Math.Abs(buf[i+2]-r) <= tol && Math.Abs(buf[i+1]-g) <= tol && Math.Abs(buf[i]-b) <= tol) n++;
            }
            return n;
        }
    }
}
"@
$script:dot = [string][char]0x25CF
function Dirty { (Title).Contains($script:dot) }
# Count pixels of colour (r,g,b) in a region given in 2000x1070 display coordinates, from a fresh screenshot.
function Count($r, $g, $b, $x0, $y0, $x1, $y1) { $p = Shot "_count"; [Pix]::Count($p, $r, $g, $b, 45, $x0, $y0, $x1, $y1, 2000.0) }
$script:RED = @(224,52,52); $script:BLUE = @(32,96,224)
function CountRed($x0,$y0,$x1,$y1) { Count 224 52 52 $x0 $y0 $x1 $y1 }
function CountBlue($x0,$y0,$x1,$y1) { Count 32 96 224 $x0 $y0 $x1 $y1 }
# Choose a tool the way a person does: press its button; its panel opens (all but the pointer), so put it away.
function UseTool($name) { Press $name; if ($name -ne 'laser') { Tap 0.95 0.95 } }
# A barrel-button lasso round a rectangle (the flag only lasts within one bot call).
function Lasso($x0, $y0, $x1, $y1) { B flag barrel on rect $x0 $y0 $x1 $y1 8 }
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @"
using System; using System.Drawing; using System.Drawing.Imaging; using System.Runtime.InteropServices;
public static class Pix2 {
    static byte[] Load(string path, out int w, out int h, out int stride) {
        using (var bmp = new Bitmap(path)) {
            w = bmp.Width; h = bmp.Height;
            var d = bmp.LockBits(new Rectangle(0,0,w,h), ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
            stride = d.Stride; byte[] buf = new byte[Math.Abs(d.Stride) * h];
            Marshal.Copy(d.Scan0, buf, 0, buf.Length); bmp.UnlockBits(d); return buf;
        }
    }
    static bool Hit(byte[] buf, int w, int h, int stride, int x, int y, int r, int cr, int cg, int cb, int tol) {
        for (int yy = Math.Max(0,y-r); yy <= Math.Min(h-1,y+r); yy++) for (int xx = Math.Max(0,x-r); xx <= Math.Min(w-1,x+r); xx++) {
            int i = yy*stride + xx*4;
            if (Math.Abs(buf[i+2]-cr) <= tol && Math.Abs(buf[i+1]-cg) <= tol && Math.Abs(buf[i]-cb) <= tol) return true;
        }
        return false;
    }
    // How many of n sample points along the segment (display coordinates, dispW wide) have a pixel of that colour within r display px.
    public static int Along(string path, double x0, double y0, double x1, double y1, int n, double r, double dispW, int cr, int cg, int cb, int tol) {
        int w, h, stride; byte[] buf = Load(path, out w, out h, out stride); double k = w / dispW; int hits = 0;
        for (int s = 0; s < n; s++) {
            double t = 0.04 + 0.92 * s / Math.Max(1, n - 1);
            int x = (int)((x0 + (x1 - x0) * t) * k), y = (int)((y0 + (y1 - y0) * t) * k);
            if (Hit(buf, w, h, stride, x, y, (int)Math.Ceiling(r * k), cr, cg, cb, tol)) hits++;
        }
        return hits;
    }
    // The bounding box [x0,y0,x1,y1] (display coordinates) of pixels of that colour inside a region; -1s if there are none.
    public static double[] BBox(string path, double rx0, double ry0, double rx1, double ry1, double dispW, int cr, int cg, int cb, int tol) {
        int w, h, stride; byte[] buf = Load(path, out w, out h, out stride); double k = w / dispW;
        int ax = Math.Max(0,(int)(rx0*k)), ay = Math.Max(0,(int)(ry0*k)), bx = Math.Min(w,(int)(rx1*k)), by = Math.Min(h,(int)(ry1*k));
        int x0 = int.MaxValue, y0 = int.MaxValue, x1 = -1, y1 = -1;
        for (int y = ay; y < by; y++) for (int x = ax; x < bx; x++) {
            int i = y*stride + x*4;
            if (Math.Abs(buf[i+2]-cr) <= tol && Math.Abs(buf[i+1]-cg) <= tol && Math.Abs(buf[i]-cb) <= tol) { if (x<x0) x0=x; if (y<y0) y0=y; if (x>x1) x1=x; if (y>y1) y1=y; }
        }
        if (x1 < 0) return new double[] { -1, -1, -1, -1 };
        return new double[] { x0 / k, y0 / k, x1 / k, y1 / k };
    }
}
"@
$script:last = $null
# After this, a fresh screenshot is taken once and the pixel helpers below read it.
function Snap { $script:last = Shot "_snap" }
# Fraction (0..1) of a segment, in display coordinates, that is covered by pixels of a colour (default: the pen black).
function Straight($x0,$y0,$x1,$y1,[int]$r=28,[int]$g=30,[int]$b=36) { [Pix2]::Along($script:last,$x0,$y0,$x1,$y1,40,3.0,2000.0,$r,$g,$b,45) / 40.0 }
function Extent($x0,$y0,$x1,$y1,[int]$r=28,[int]$g=30,[int]$b=36) { [Pix2]::BBox($script:last,$x0,$y0,$x1,$y1,2000.0,$r,$g,$b,45) }
function Accent($x0,$y0,$x1,$y1) { Count 64 132 255 $x0 $y0 $x1 $y1 }
function Near($a,$b,$tol=6) { [math]::Abs($a - $b) -le $tol }
function Dbl($x,$y) { B tap $x $y tap $x $y }
function BarrelTap($x,$y) { B flag barrel on tap $x $y }