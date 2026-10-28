. "$PSScriptRoot\lib.ps1"
$script:UseMouse = $true
Launch "tool text`ntext-slot 0`ntext-size 40.00`n" "" "x"
function Dark($x0,$y0,$x1,$y1) { Count 28 30 36 $x0 $y0 $x1 $y1 }
# --- near the right edge: a long text is moved back onto the page when it is finished
Tap 0.85 0.2; Type-Text "This text is far too long to fit here"
Tap 0.1 0.9; Keys "{ESC}"
Shot "a6-1-right-edge" | Out-Null
Check ((Dark 1986 100 2000 400) -eq 0) "nothing drawn beyond the right edge of the page"
Check ((Dark 1000 200 1990 300) -gt 500) "the text is on the page"
# --- near the bottom edge
Tap 0.3 0.97; Type-Text "Low"; 
Keys "{ENTER}two{ENTER}three"
Tap 0.9 0.5; Keys "{ESC}"
Shot "a6-2-bottom-edge" | Out-Null
# --- accents and symbols (the font has no tick or CJK glyphs)
Tap 0.1 0.6; Type-Text "héllo wörld ✓ 日本語 ñ ü ß"
Tap 0.9 0.5; Keys "{ESC}"
Shot "a6-3-unicode" | Out-Null
"alive after unicode: " + (Alive) + " errors: " + (Errs)
# --- a long burst of typing is kept up with
$long = ("abcdefghij " * 25)
$sw = [Diagnostics.Stopwatch]::StartNew()
Tap 0.1 0.35; Type-Text $long
"typed $($long.Length) chars in $([int]$sw.Elapsed.TotalMilliseconds) ms; alive: $(Alive)"
Tap 0.9 0.5; Keys "{ESC}"
Shot "a6-4-long" | Out-Null
Check ((Dark 1986 100 2000 1070) -eq 0) "the long text did not run off the right edge"
Check ((Dark 0 340 1990 480) -gt 1500) "...it is wrapped onto several lines"
# --- the biggest size
Press text
Tap 0.559 0.1635
Tap 0.95 0.95
Tap 0.05 0.5; Type-Text "BIG"
Tap 0.9 0.8; Keys "{ESC}"
Shot "a6-5-big" | Out-Null
"errors: " + (Errs); "alive: " + (Alive)
Stop-App
