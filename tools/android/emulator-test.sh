#!/usr/bin/env bash
# Runs the APK on a running Android emulator (or a device on adb) and leaves what it saw in a folder: screenshots, logcat and a
# verdict. The checks are few and coarse, on purpose: the app starts and keeps running, it drew its first frame, a tap on the toolbar
# opens the pages overview (so buttons are pressed by a finger), a stylus swipe leaves ink and a finger swipe does not (so a stylus
# is told from a finger), and the save button opens the Save As card.
#
#   tools/android/emulator-test.sh path/to/inkpdf.apk [out-folder]
#
# The toolbar's buttons are where they are at 100% scale times the screen's density (the left pill: close, undo, redo, save, delete
# page, add page, pages overview at x = 39, 86, 125, 163, 211, 249, 287; y = 39), until the app can dump its own layout (roadmap F2).
set -uo pipefail

apk="${1:?the APK to install}"
out="${2:-emulator-out}"
mkdir -p "$out"
fail=0
check() { if eval "$2"; then echo "ok:   $1"; else echo "FAIL: $1"; fail=1; fi; }

adb wait-for-device
adb install -r "$apk" | tail -1
adb logcat -c
adb shell am start -n app.inkpdf.test/android.app.NativeActivity >/dev/null
sleep 20

shot() { adb exec-out screencap -p >"$out/$1.png"; }
alive() { [ -n "$(adb shell pidof app.inkpdf.test | tr -d '\r')" ]; }
dpi="$(adb shell wm density | grep -o '[0-9]\+' | tail -1)"
size="$(adb shell wm size | grep -o '[0-9]\+x[0-9]\+' | tail -1)"
w="${size%x*}"; h="${size#*x}"
at() { awk -v v="$1" -v d="$dpi" 'BEGIN { printf "%d", v * d / 160 }'; }   # a place at 100% scale, in pixels
echo "screen ${w}x${h} at ${dpi} dpi"

shot 1-start
check "the app is running after it started" alive

# A finger taps the pages overview button: it opens, so the screen changes.
adb shell input tap "$(at 287)" "$(at 39)"; sleep 3; shot 2-overview
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
adb shell input tap "$(at 163)" "$(at 39)"; sleep 3; shot 6-save-as
check "the save button changed the screen (the Save As card)" "! cmp -s '$out/5-stylus.png' '$out/6-save-as.png'"
check "the app is still running at the end" alive

adb logcat -d -s inkpdf >"$out/logcat-inkpdf.txt"
adb logcat -d >"$out/logcat-all.txt"
check "the app reported its GPU" "grep -q 'GPU:' '$out/logcat-inkpdf.txt'"
check "nothing panicked" "! grep -q 'panicked' '$out/logcat-inkpdf.txt'"
echo "$([ $fail = 0 ] && echo PASS || echo FAIL)" >"$out/verdict.txt"
cat "$out/verdict.txt"
exit "$fail"
