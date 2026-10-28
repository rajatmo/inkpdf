# The few helpers the real-input checks need: start the built app, give it a stylus and finger (inkpdf-bot), the real mouse
# and keyboard, take a screenshot and count pixels of a colour. APPDATA always points into a scratch folder, never the
# user's settings. Nothing is written inside the repository: scratch files go to `inkpdf-e2e` beside it (or
# INKPDF_E2E_DIR), and the release build under test is read from `inkpdf-build` beside it (CARGO_TARGET_DIR if you build
# elsewhere): `cargo build --release -p inkpdf`.
$script:repo = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$script:workspace = Split-Path -Parent $script:repo
$script:sp = if ($env:INKPDF_E2E_DIR) { $env:INKPDF_E2E_DIR } else { Join-Path $script:workspace "inkpdf-e2e" }
New-Item -ItemType Directory -Force $script:sp | Out-Null
$script:target = Join-Path $(if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $script:workspace "inkpdf-build" }) "release"
$script:bot = "$script:target\inkpdf-bot.exe"

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
    // Pixels within `tol` of (r,g,b) in a rectangle of a PNG; the rectangle is in display units `dispW` wide.
    public static int Count(string path, int r, int g, int b, int tol, double x0, double y0, double x1, double y1, double dispW) {
        using (var bmp = new Bitmap(path)) {
            double k = bmp.Width / dispW;
            int ax = Math.Max(0, (int)(x0 * k)), ay = Math.Max(0, (int)(y0 * k)), bx = Math.Min(bmp.Width, (int)(x1 * k)), by = Math.Min(bmp.Height, (int)(y1 * k));
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
# Start the app on a blank document, or on a copy of the PDF `$Pdf` (a name in the scratch folder, or "pages3": a
# three-page PDF made by the build's own generator), with the settings file `$Settings` and extra arguments.
function Launch([string]$Settings = "", [string]$Pdf = "", [string[]]$AppArgs = @()) {
    Get-Process inkpdf -ErrorAction SilentlyContinue | Stop-Process -Force
    $env:APPDATA = Join-Path $script:sp "appdata"
    Remove-Item "$env:APPDATA\InkPDF" -Recurse -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force "$env:APPDATA\InkPDF" | Out-Null
    if ($Settings) { Set-Content "$env:APPDATA\InkPDF\settings.txt" $Settings -Encoding ascii }
    $script:file = $null
    if ($Pdf) {
        $script:file = Join-Path $script:sp "work-$Pdf.pdf"
        if ($Pdf -match "^pages(\d+)$") { & "$script:target\inkpdf-gen.exe" $script:file $Matches[1] | Out-Null }
        else { Copy-Item (Join-Path $script:sp $Pdf) $script:file -Force }
    }
    $script:appArgs = @($AppArgs)
    Start-App
}
function Start-App {
    $a = @(if ($script:file) { "`"$script:file`"" }) + @("--topmost") + @($script:appArgs)
    $script:app = Start-Process "$script:target\inkpdf.exe" -ArgumentList $a -PassThru -RedirectStandardOutput (Join-Path $script:sp "app.out.txt") -RedirectStandardError (Join-Path $script:sp "app.err.txt")
    # Ready when the window has a title (not a fixed wait).
    foreach ($i in 1..60) { Start-Sleep -Milliseconds 150; $script:app.Refresh(); if ($script:app.MainWindowTitle) { break } }
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

# ---- Input. Positions are fractions of the window.
function B { & $script:bot $script:name @args | Out-Null; Start-Sleep -Milliseconds 120 }   # stylus and finger
function Px($fx, $fy) { @([int]($script:ox + $fx * $script:cw), [int]($script:oy + $fy * $script:ch)) }
function Mouse-Drag($fx0, $fy0, $fx1, $fy1, [string]$button = "left") {
    $down = @{ left = 2; right = 8; middle = 32 }[$button]; $up = @{ left = 4; right = 16; middle = 64 }[$button]
    $p = Px $fx0 $fy0; $q = Px $fx1 $fy1; [void][W]::SetCursorPos($p[0], $p[1]); Start-Sleep -Milliseconds 80
    [W]::mouse_event($down, 0, 0, 0, [UIntPtr]::Zero)
    foreach ($i in 1..12) { [void][W]::SetCursorPos([int]($p[0] + ($q[0] - $p[0]) * $i / 12), [int]($p[1] + ($q[1] - $p[1]) * $i / 12)); Start-Sleep -Milliseconds 10 }
    [W]::mouse_event($up, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 150
}
function Click($fx, $fy) { Mouse-Drag $fx $fy $fx $fy }
function Type-Text([string]$s) {
    [System.Windows.Forms.SendKeys]::SendWait(($s.ToCharArray() | ForEach-Object { if ('+^%~(){}[]'.Contains($_)) { "{$_}" } else { "$_" } }) -join "")
    Start-Sleep -Milliseconds 150
}
function Keys([string]$k) { [System.Windows.Forms.SendKeys]::SendWait($k); Start-Sleep -Milliseconds 150 }
# Toolbar buttons (window fractions, a maximised 4K window): close undo redo save | delete page, add page | pen shape text pointer | mode, fullscreen
$script:btn = @{ close = 0.016; undo = 0.0365; redo = 0.0545; save = 0.0715; del = 0.093; plus = 0.1102; pen = 0.4745; shape = 0.4915; text = 0.5085; laser = 0.5255; mode = 0.9664; full = 0.9835 }
function Press($name) { Click $script:btn[$name] 0.031 }
# The same button with the stylus, or with a finger.
function Pen-Press($name) { B tap $script:btn[$name] 0.031 }
function Finger-Press($name) { B finger down 1 $script:btn[$name] 0.031; B finger up 1 $script:btn[$name] 0.031 }
# Choose a tool the way a person does: press its button (its panel opens) and tap the page to put the panel away.
function Use-Tool($name) { Press $name; if ($name -ne "laser") { Click 0.95 0.95 } }

# ---- Looking
function Shot([string]$n) {
    $script:app.Refresh(); $h = $script:app.MainWindowHandle
    $c = New-Object W+RECT; [void][W]::GetClientRect($h, [ref]$c)
    $pt = New-Object W+POINT; [void][W]::ClientToScreen($h, [ref]$pt)
    $bmp = New-Object System.Drawing.Bitmap ($c.Right - $c.Left), ($c.Bottom - $c.Top)
    $g = [System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($pt.X, $pt.Y, 0, 0, $bmp.Size)
    $path = Join-Path $script:sp "$n.png"; $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose(); $path
}
# Pixels of a colour in a region (display units: the window is 2000 x 1070), from a fresh screenshot. The page area by default.
function Count($r, $g, $b, $x0 = 300, $y0 = 150, $x1 = 1900, $y1 = 1000) { [W]::Count((Shot "look"), $r, $g, $b, 45, $x0, $y0, $x1, $y1, 2000.0) }
function Black { Count 28 30 36 }       # the pen's default ink
function Red { Count 224 52 52 }        # settings `slot 2` / `text-slot 2`
function Accent($x0, $y0, $x1, $y1) { Count 64 132 255 $x0 $y0 $x1 $y1 }

# ---- Windows telling the app about its settings (posture and others), as the system does.
function Tell-Setting([string]$name) {
    $r = [IntPtr]::Zero; [void][W]::SendMessageTimeout($script:app.MainWindowHandle, 0x1A, [IntPtr]::Zero, $name, 2, 3000, [ref]$r); Start-Sleep -Milliseconds 500
}

# ---- Reporting
function Check($cond, $m) { if ($cond) { Write-Host "ok:   $m" } else { Write-Host "FAIL: $m" -ForegroundColor Red } }
