# The two modes, with real input against the real app. Desktop: the mouse stands in for a stylus (left tip, right
# eraser end, middle barrel button) and Ctrl+Z / Ctrl+Y work. Tablet: the mouse only scrolls, the stylus places
# text, Ctrl does nothing. Also the posture: the default follows the machine (a desktop PC here), a
# "ConvertibleSlateMode" setting-change message makes the app look again, and a mode given on the command line
# is not changed by it.
. "$PSScriptRoot\lib.ps1"
$script:UseMouse = $true
$black = @(28, 30, 36)
function Ink { Count $black[0] $black[1] $black[2] 300 150 1900 1000 }

function Case([string]$mode, [string]$tool, [string]$name, [scriptblock]$do, [bool]$wantDirty) {
    $script:extraArgs = if ($mode) { @("--mode", $mode) } else { @() }
    $shape = if ($tool -eq "geometry") { "shape line`n" } else { "" }
    Launch "tool $tool`n$shape" "" "x"
    & $do
    Start-Sleep -Milliseconds 300
    $label = if ($mode) { $mode } else { "default" }
    Check (((Dirty) -eq $wantDirty) -and (Alive)) "$label / $tool / $name -> changed=$(Dirty), wanted $wantDirty"
    Stop-App
}

# ---- Desktop: the mouse draws.
Case "desktop" "pen" "left drag draws" { Mouse-Drag 0.3 0.5 0.7 0.55 } $true
Case "desktop" "pen" "right drag on empty page erases nothing" { Mouse-DragButton 0.3 0.5 0.7 0.5 "right" } $false
Case "desktop" "pen" "middle drag (lasso) on empty page changes nothing" { Mouse-DragButton 0.3 0.4 0.7 0.7 "middle" } $false
Case "desktop" "geometry" "left drag makes a shape" { Mouse-Drag 0.3 0.4 0.6 0.6 } $true
Case "desktop" "laser" "left drag only points" { Mouse-Drag 0.3 0.4 0.6 0.6 } $false
Case "desktop" "pen" "wheel scrolls" { Wheel 3; Wheel -3 } $false
Case "desktop" "pen" "a finger pans and zooms" { B drag 0.5 0.4 0.5 0.8 20; B pinch 0.5 0.5 0.05 0.2 20 } $false
Case "desktop" "pen" "typing with no text box does nothing" { Type-Text "hello" } $false
Case "desktop" "text" "mouse click, typing, click away makes text" { Mouse-Click 0.4 0.5; Type-Text "ab"; Mouse-Click 0.8 0.8 } $true
Case "desktop" "text" "the stylus does nothing to the page" { B tap 0.4 0.5; Type-Text "ab"; B tap 0.8 0.8 } $false

# ---- Desktop: the right button erases what the left drew, and Ctrl+Z / Ctrl+Y undo and redo.
Launch "tool pen`n" "" "x"
Mouse-Drag 0.3 0.5 0.7 0.5
$drawn = Ink
Check ($drawn -gt 500) "desktop: the mouse drew a stroke ($drawn black pixels)"
Mouse-DragButton 0.5 0.44 0.5 0.56 "right"
$after = Ink
Check ($after -eq 0) "desktop: the right button erased it ($after left)"
Keys "^z"
Check ((Ink) -gt 500) "desktop: Ctrl+Z brought it back"
Keys "^y"
Check ((Ink) -eq 0) "desktop: Ctrl+Y took it away again"
Keys "^z"
$back = Ink
Mouse-DragButton 0.2 0.3 0.8 0.7 "middle"
Check ((Ink) -eq $back -and (Alive)) "desktop: a middle-button lasso draws nothing"
Stop-App

# ---- Tablet: the mouse only scrolls, the stylus places text, Ctrl does nothing.
Case "tablet" "pen" "left drag draws nothing" { Mouse-Drag 0.3 0.5 0.7 0.55 } $false
Case "tablet" "pen" "right drag draws nothing" { Mouse-DragButton 0.3 0.5 0.7 0.5 "right" } $false
Case "tablet" "pen" "stylus draws" { B stroke 0.3 0.5 0.7 0.55 0.02 2 40 } $true
Case "tablet" "geometry" "left drag makes no shape" { Mouse-Drag 0.3 0.4 0.6 0.6 } $false
Case "tablet" "geometry" "stylus makes a shape" { B line 0.3 0.4 0.6 0.6 20 } $true
Case "tablet" "laser" "mouse only scrolls" { Mouse-Drag 0.3 0.4 0.6 0.6 } $false
Case "tablet" "pen" "a finger pans and zooms" { B drag 0.5 0.4 0.5 0.8 20; B pinch 0.5 0.5 0.05 0.2 20 } $false
Case "tablet" "text" "stylus tap, typing, tap away makes text" { B tap 0.4 0.5; Type-Text "ab"; B tap 0.8 0.8 } $true
Case "tablet" "text" "mouse click does not place text" { Mouse-Click 0.4 0.5; Type-Text "ab"; Mouse-Click 0.8 0.8 } $false

Launch "tool pen`n" "" "x"
$script:extraArgs = @()
Stop-App
$script:extraArgs = @("--mode", "tablet")
Launch "tool pen`n" "" "x"
B stroke 0.3 0.5 0.7 0.5 0.0 1 40
$drawn = Ink
Keys "^z"
Check ((Ink) -eq $drawn -and $drawn -gt 500) "tablet: Ctrl+Z does nothing ($drawn black pixels kept)"
Stop-App

# ---- Posture: the default follows the machine; a setting-change message makes the app look again.
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices;
public static class Msg {
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    public static extern IntPtr SendMessageTimeout(IntPtr h, uint msg, IntPtr w, string l, uint flags, uint timeout, out IntPtr result);
}
"@
function Modes-Printed { ,@(Get-Content (Join-Path $script:sp "app.err.txt") | Where-Object { $_ -match "^mode:" }) }
function Tell-Posture-Changed { $r = [IntPtr]::Zero; [void][Msg]::SendMessageTimeout($script:app.MainWindowHandle, 0x1A, [IntPtr]::Zero, "ConvertibleSlateMode", 2, 3000, [ref]$r); Start-Sleep -Milliseconds 700 }
function Tell-Other-Setting { $r = [IntPtr]::Zero; [void][Msg]::SendMessageTimeout($script:app.MainWindowHandle, 0x1A, [IntPtr]::Zero, "ImmersiveColorSet", 2, 3000, [ref]$r); Start-Sleep -Milliseconds 700 }

$env:INKPDF_TRACE = "1"
$script:extraArgs = @()
Launch "tool pen`n" "" "x"
$printed = Modes-Printed
Check ($printed.Count -eq 1 -and $printed[0] -match "Desktop") "default: this desktop PC starts in the desktop mode ($($printed -join ' | '))"
Mouse-Drag 0.3 0.5 0.7 0.55
Check (Dirty) "default: so its mouse draws"
Tell-Other-Setting
Check ((Modes-Printed).Count -eq 1) "an unrelated setting change makes the app do nothing"
Tell-Posture-Changed
$printed = Modes-Printed
Check ($printed.Count -eq 2 -and $printed[1] -match "Desktop") "a posture change makes the app look again, and a desktop is still a desktop ($($printed -join ' | '))"
Check (Alive) "and it carries on"
Stop-App

$script:extraArgs = @("--mode", "tablet")
Launch "tool pen`n" "" "x"
Tell-Posture-Changed
$printed = Modes-Printed
Check ($printed.Count -eq 1 -and $printed[0] -match "Tablet") "a mode given on the command line is not changed by the posture ($($printed -join ' | '))"
Stop-App
Remove-Item Env:\INKPDF_TRACE
"errors: " + (Errs)
