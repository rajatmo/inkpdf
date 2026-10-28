# The device rules, for every tool, with real inputs against the real app: a fresh blank document per case,
# one kind of input, then whether the document became changed (the title's dot) must match the rule.
. "$PSScriptRoot\lib.ps1"
$script:UseMouse = $true

# (The eraser end is not here: Windows' synthetic pen injector drops the eraser flag on this OS build, so the
# app sees an ordinary pen; the eraser is covered by the unit and seeded stress tests.)
# Each action: a name, what to do, and for which tools it must change the document.
$actions = @(
    @{ n = "stylus tip stroke";      do = { B stroke 0.3 0.4 0.6 0.5 0.03 2 40 };                     dirty = @("pen", "shape") },
    @{ n = "stylus tip drag (straight)"; do = { B line 0.3 0.4 0.6 0.55 25 };                           dirty = @("pen", "shape") },
    @{ n = "stylus barrel lasso";    do = { Lasso 0.3 0.3 0.6 0.6 };                                  dirty = @() },
    @{ n = "stylus taps";            do = { B tap 0.4 0.5 tap 0.5 0.5 };                              dirty = @("pen") },
    @{ n = "mouse click";            do = { Mouse-Click 0.4 0.5 };                                    dirty = @() },
    @{ n = "mouse drag";             do = { Mouse-Drag 0.3 0.4 0.6 0.5 };                             dirty = @() },
    @{ n = "mouse wheel";            do = { Wheel 3; Wheel -3 };                                      dirty = @() },
    @{ n = "finger drag";            do = { B drag 0.5 0.4 0.5 0.8 20 };                              dirty = @() },
    @{ n = "finger pinch";           do = { B pinch 0.5 0.5 0.05 0.2 20 };                            dirty = @() },
    @{ n = "keyboard typing";        do = { Type-Text "hello world"; Keys "{ENTER}{BACKSPACE}" };      dirty = @() },
    @{ n = "mouse click then typing then click away"; do = { Mouse-Click 0.4 0.5; Type-Text "ab"; Mouse-Click 0.8 0.8 }; dirty = @("text") }
)
$tools = @("pen", "shape", "text", "laser")
$fail = 0; $total = 0
foreach ($tool in $tools) {
    $settings = "tool " + $(if ($tool -eq "shape") { "geometry" } else { $tool }) + "`nshape line`n"
    foreach ($a in $actions) {
        Launch $settings "" "x"
        $before = Dirty
        & $a.do
        Start-Sleep -Milliseconds 300
        $want = $a.dirty -contains $tool
        $got = Dirty
        $total++
        $okay = ($got -eq $want) -and (Alive) -and (-not $before)
        if ($okay) { Ok "$tool / $($a.n): changed=$got" } else { $fail++; Fail "$tool / $($a.n): changed=$got, wanted $want (alive $(Alive))" }
        Stop-App
    }
}
"cases: $total, failures: $fail"
"errors: " + (Errs)
