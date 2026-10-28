. "$PSScriptRoot\lib.ps1"
$script:UseMouse = $true
Launch "tool text`ntext-slot 0`ntext-size 40.00`n" "" "x"
$sw = @(0.4535, 0.481, 0.5085, 0.536, 0.5635); $swy = 0.2019; $sly = 0.1635
# --- type a box, then style it from the panel while the cursor is still in it
Tap 0.2 0.4
Type-Text "Style me"
Check ((Count 28 30 36 395 380 700 470) -gt 200) "starts in the saved colour (black)"
Press text
Tap $sw[1] $swy
Check ((CountBlue 395 380 700 470) -gt 200) "picking blue recolours the box being typed"
Check ((Count 28 30 36 395 380 700 470) -lt 30) "...and nothing is black any more"
Check ((CountBlue 395 380 700 470) -gt 200) "still typing after using the panel"
Type-Text "!"
# size slider: far right = biggest, far left = smallest
Tap 0.558 $sly
$big = CountBlue 300 150 1900 1000
Tap 0.459 $sly
$small = CountBlue 300 150 1900 1000
Check ($big -gt 5 * $small -and $small -gt 0) "size slider: right end is much bigger than left end ($big vs $small blue px)"
# middle
Tap 0.5085 $sly
# click the page: the first press only puts the panel away; the box is still being typed into
Tap 0.7 0.8
Type-Text "x"
Check (-not (Dirty)) "the press that closed the panel did not end the typing"
Tap 0.7 0.8
Check (Dirty) "the next press on the page ended it"
Keys "{ESC}"
$settings = Get-Content "$env:APPDATA\InkPDF\settings.txt" -Raw
Check ($settings -match "text-slot 1") "the colour choice was saved ($($settings -replace "`n",' | ')) "
Check ($settings -match "text-size \d") "the size was saved"
# --- switching tools ends typing, keyboard in other tools does nothing
UseTool text
Tap 0.3 0.7
Type-Text "tool switch"
UseTool pen
Check ((CountBlue 295 690 1200 800) -gt 100) "switching tools kept what was typed"
$before = CountBlue 0 100 2000 1070
Type-Text "should not appear"; Keys "{ENTER}{BACKSPACE}{ESC}"
Check ((CountBlue 0 100 2000 1070) -eq $before) "the pen tool ignores the keyboard"
UseTool shape
Type-Text "nor here"
Check ((CountBlue 0 100 2000 1070) -eq $before) "the shape tool ignores the keyboard"
UseTool laser
Type-Text "nor here"
Check ((CountBlue 0 100 2000 1070) -eq $before) "the pointer tool ignores the keyboard"
UseTool text
Type-Text "nor with no open box"
Check ((CountBlue 0 100 2000 1070) -eq $before) "the text tool with no open box ignores the keyboard"
"errors: " + (Errs); "alive: " + (Alive)
Shot "a2-final" | Out-Null
Stop-App
