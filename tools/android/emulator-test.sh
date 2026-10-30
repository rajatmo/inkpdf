#!/usr/bin/env bash
# Runs the APK on a running Android emulator (or a device on adb) and leaves what it saw in a folder: screenshots, logcat and a
# verdict. The checks are few and coarse, on purpose: the app starts and keeps running, it drew its first frame, a tap on the toolbar
# opens the pages overview (so buttons are pressed by a finger), a stylus swipe leaves ink and a finger swipe does not (so a stylus
# is told from a finger), and the save button opens the Save As card.
#
#   tools/android/emulator-test.sh path/to/inkpdf.apk [out-folder]
#
# The toolbar's buttons are found by name: the app writes where each one is (the file --layout makes on a desktop) into its own folder when a file
# layout.on is there (db shell touch), and this reads it. On a narrow screen the left pill is folded into a round button (old-doc): it is
# pressed first, as a person would.
set -uo pipefail
# (Git Bash on Windows would turn an address such as /sdcard/... in an argument into a Windows path.)
export MSYS_NO_PATHCONV=1

apk="${1:?the APK to install}"
out="${2:-emulator-out}"
mkdir -p "$out"
fail=0
check() { if eval "$2"; then echo "ok:   $1"; else echo "FAIL: $1"; fail=1; fi; }

adb wait-for-device
adb install -r "$apk" | tail -1
adb shell am force-stop app.inkpdf.test
adb logcat -c
data=/sdcard/Android/data/app.inkpdf.test/files
adb shell "mkdir -p $data; rm -f $data/layout.json; touch $data/layout.on" >/dev/null 2>&1
# (The launcher is the Java `Host` activity since the file picker and clipboard came in: a NativeActivity of its own.)
adb shell am start -n app.inkpdf.test/.Host >/dev/null
sleep 20

shot() { adb exec-out screencap -p >"$out/$1.png"; }
alive() { [ -n "$(adb shell pidof app.inkpdf.test | tr -d '\r')" ]; }
dpi="$(adb shell wm density | grep -o '[0-9]\+' | tail -1)"
size="$(adb shell wm size | grep -o '[0-9]\+x[0-9]\+' | tail -1)"
w="${size%x*}"; h="${size#*x}"
# Where a button is, in pixels (x y), from the layout the app wrote; empty if there is no such button now.
spot() { adb shell cat "$data/layout.json" 2>/dev/null | tr -d '\r' | grep -o "\"$1\":\[[0-9.]*,[0-9.]*\]" | head -1 | sed 's/.*\[\([0-9.]*\),\([0-9.]*\)\]/\1 \2/'; }
tap_at() { adb shell input tap $(echo "$1" | awk '{printf "%d %d", $1, $2}'); }
# Press a toolbar button by name, unfolding the left pill first if the button is inside a folded one.
press() {
    local p; p="$(spot "$1")"
    if [ -z "$p" ]; then
        local f; f="$(spot fold-doc)"
        [ -n "$f" ] && { tap_at "$f"; for _ in 1 2 3 4 5; do sleep 1; p="$(spot "$1")"; [ -n "$p" ] && break; done; }
    fi
    [ -n "$p" ] && tap_at "$p"
}
echo "screen ${w}x${h} at ${dpi} dpi"

shot 1-start
check "the app is running after it started" alive

# A finger taps the pages overview button: it opens, so the screen changes.
press pages; sleep 3; shot 2-overview
check "a finger's tap on a toolbar button did something (the overview opened)" "! cmp -s '$out/1-start.png' '$out/2-overview.png'"
# Back puts the card away (Back is Escape).
adb shell input keyevent KEYCODE_BACK; sleep 2; shot 3-back
check "back put it away again" "cmp -s '$out/1-start.png' '$out/3-back.png' || ! cmp -s '$out/2-overview.png' '$out/3-back.png'"

# A finger swipe over the page pans it and leaves no ink; a stylus swipe draws.
cx=$((w / 2)); cy=$((h / 2))
adb shell input touchscreen swipe "$cx" "$((cy + 300))" "$cx" "$((cy - 100))" 300; sleep 2; shot 4-finger
adb shell input stylus swipe "$((cx - 250))" "$cy" "$((cx + 250))" "$((cy + 60))" 500; sleep 2; shot 5-stylus
check "a stylus swipe changed the screen (ink)" "! cmp -s '$out/4-finger.png' '$out/5-stylus.png'"

# The save button opens the Save As card.
press save; sleep 3; shot 6-save-as
check "the save button changed the screen (the Save As card)" "! cmp -s '$out/5-stylus.png' '$out/6-save-as.png'"
check "the app is still running at the end" alive

adb logcat -d -s inkpdf >"$out/logcat-inkpdf.txt"
adb logcat -d >"$out/logcat-all.txt"
check "the app reported its GPU" "grep -q 'GPU:' '$out/logcat-inkpdf.txt'"
check "nothing panicked" "! grep -q 'panicked' '$out/logcat-inkpdf.txt'"
echo "$([ $fail = 0 ] && echo PASS || echo FAIL)" >"$out/verdict.txt"
cat "$out/verdict.txt"
exit "$fail"
