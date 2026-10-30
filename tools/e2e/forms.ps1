# A form, for real: that the boxes of a PDF form are filled with the real mouse and keyboard (the Select tool: a click, and typed text), with the real stylus
# and finger, that what was put in is saved into the file, and is there when the file is opened again. What every kind of field does with every device is for the
# unit tests; this proves the real window reaches them and that the saved file draws the values.
. "$PSScriptRoot\lib.ps1"
# Dark pixels: the program's own marks (24 26 34), the file's (black), and the page's text and frames, which are there before and after.
function Dark { (Count 24 26 34) + (Count 0 0 0) }

Launch "tool select`n" "form" @("--mode", "desktop")
Check (-not (Dirty)) "a form opens as it is: nothing changed"
$empty = Dark
# The mouse (left button) with the Select tool.
Click-Spot (Spot fields agree); Start-Sleep -Milliseconds 300
Check (Dirty) "a click on the check box fills it and marks the document changed"
Check ((Dark) -gt $empty) "and the mark is drawn ($empty -> $(Dark) dark pixels)"
Click-Spot (Spot fields "size#1"); Start-Sleep -Milliseconds 300
Click-Spot (Spot fields colour); Start-Sleep -Milliseconds 300
$chosen = Dark
Click-Spot (Spot fields name); Start-Sleep -Milliseconds 300
Type-Text "Ada Lovelace"; Start-Sleep -Milliseconds 300
Check ((Dark) -gt $chosen) "typed text is drawn in the field ($chosen -> $(Dark) dark pixels)"
Keys "{ENTER}"; Start-Sleep -Milliseconds 200
# The stylus's tip (a tap) and a finger: each turns the check box, so it is on again after the three.
Tap-Spot (Spot fields agree); Start-Sleep -Milliseconds 300
$p = Spot fields agree
B finger down 1 ($p[0] / $script:cw) ($p[1] / $script:ch); B finger up 1 ($p[0] / $script:cw) ($p[1] / $script:ch); Start-Sleep -Milliseconds 300
Check (Healthy) "no crash, nothing printed to stderr"
$filled = Dark
# Saved, and there when the file is opened again (the file's own appearances draw it, not the program).
Keys "^s"
foreach ($i in 1..30) { if (-not (Dirty)) { break }; Start-Sleep -Milliseconds 200 }
Check (-not (Dirty)) "the document is saved"
Reopen
Start-Sleep -Milliseconds 800
Check (-not (Dirty)) "reopened: nothing to save"
Check ((Dark) -gt $empty) "and the values are drawn by the file ($empty -> $(Dark) dark pixels; filled was $filled)"
Check (Healthy) "no crash after reopening"
Stop-App
