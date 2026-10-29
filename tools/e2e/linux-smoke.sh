#!/usr/bin/env bash
# Linux counterpart of smoke.ps1, small on purpose: starts the built app on a virtual X display (software Vulkan) and
# checks two things a person does:
#
#  1. a PDF is opened, drawn on with the mouse (desktop mode: the left button is a stylus) and saved with Ctrl+S: the file
#     comes out with an Ink annotation of ours in it;
#  2. a new document is drawn on and saved: Ctrl+S puts up the app's own Save As dialog, which opens on the Documents folder
#     (here the one in a made-up home); a name is typed and Enter saves it there.
#
# It proves the window, the GPU, the mouse and keyboard paths, the dialog and the save on Linux; the rules themselves are
# the unit and stress tests' job.
#
#   tools/e2e/linux-smoke.sh [path/to/inkpdf] [path/to/inkpdf-gen]
#
# Needs: xvfb (xvfb-run), xdotool, mesa-vulkan-drivers (lavapipe), libxkbcommon-x11-0, a system font (fonts-liberation).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
target="${CARGO_TARGET_DIR:-$here/../../../inkpdf-build}"
app="${1:-$target/debug/inkpdf}"
gen="${2:-$target/debug/inkpdf-gen}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/home/Documents"
"$gen" "$work/t.pdf" 3 >/dev/null
cp "$work/t.pdf" "$work/original.pdf"

# The parts that run on the virtual display, as files of their own (xvfb-run runs a program, not a shell function).
cat >"$work/open.sh" <<DRIVE
set -eu
"$app" "$work/t.pdf" --mode desktop --exit-after 14 2>"$work/log.txt" &
pid=\$!
sleep 5
win="\$(xdotool search --name 'page 1' | head -1)"
[ -n "\$win" ] || { echo "FAIL: no window"; kill "\$pid"; exit 1; }
xdotool windowfocus "\$win"
sleep 0.3
xdotool mousemove 500 400
sleep 0.2
xdotool mousedown 1
for i in \$(seq 1 40); do
    xdotool mousemove \$((500 + i * 6)) \$((400 + (i % 8) * 5))
    sleep 0.02
done
xdotool mouseup 1
sleep 0.5
xdotool key ctrl+s
sleep 3
wait "\$pid"
DRIVE

cat >"$work/new.sh" <<DRIVE
set -eu
export HOME="$work/home"
unset XDG_DOCUMENTS_DIR XDG_CONFIG_HOME
"$app" --mode desktop --exit-after 20 2>"$work/log2.txt" &
pid=\$!
sleep 5
win="\$(xdotool search --name 'Untitled' | head -1)"
[ -n "\$win" ] || { echo "FAIL: no window"; kill "\$pid"; exit 1; }
xdotool windowfocus "\$win"
sleep 0.3
xdotool mousemove 500 400
sleep 0.2
xdotool mousedown 1
for i in \$(seq 1 30); do
    xdotool mousemove \$((500 + i * 6)) \$((400 + (i % 8) * 5))
    sleep 0.02
done
xdotool mouseup 1
sleep 0.5
xdotool key ctrl+s
sleep 2
xdotool type --delay 40 'Smoke test'
sleep 0.5
xdotool key Return
sleep 3
wait "\$pid"
DRIVE

# Record a short session, then play it back: the recording has the mouse's events, and the playback runs and ends by itself.
cat >"$work/record.sh" <<DRIVE
set -eu
cd "$work"
cp original.pdf r.pdf
"$app" r.pdf --mode desktop --record input.txt --exit-after 5 2>record.err &
pid=\$!
sleep 2.5
xdotool mousemove 500 400 mousedown 1 mousemove 560 430 mousemove 620 400 mouseup 1
wait "\$pid"
DRIVE
cat >"$work/play.sh" <<DRIVE
set -eu
cd "$work"
cp original.pdf r.pdf
timeout 60 "$app" r.pdf --mode desktop --replay input.txt 2>play.err
DRIVE
xvfb-run -a -s "-screen 0 1280x960x24" bash "$work/record.sh"
xvfb-run -a -s "-screen 0 1280x960x24" bash "$work/play.sh"

xvfb-run -a -s "-screen 0 1280x960x24" bash "$work/open.sh"
xvfb-run -a -s "-screen 0 1280x960x24" bash "$work/new.sh"

fail=0
check() { if eval "$2"; then echo "ok:   $1"; else echo "FAIL: $1"; fail=1; fi; }
saved="$work/home/Documents/Smoke test.pdf"
check "the app started on a GPU" "grep -q '^GPU: ' '$work/log.txt'"
check "the document was saved (the file changed)" "! cmp -s '$work/t.pdf' '$work/original.pdf'"
check "the stroke is an Ink annotation" "grep -aq '/Subtype /Ink' '$work/t.pdf'"
check "it is one of ours (inkpdf- name)" "grep -aq 'inkpdf-' '$work/t.pdf'"
check "a new document asked where to save, and saved in Documents under the name typed" "test -f '$saved'"
check "with its stroke" "grep -aq '/Subtype /Ink' '$saved'"
check "and nothing else was written there" "test \"\$(ls '$work/home/Documents' | wc -l)\" = 1"
check "the input was recorded" "grep -q 'button pressed left' '$work/input.txt'"
check "and played back to the end" "grep -q '^opened r.pdf' '$work/play.err' && ! grep -qi panic '$work/play.err'"
exit "$fail"
