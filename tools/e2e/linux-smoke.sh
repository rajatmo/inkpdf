#!/usr/bin/env bash
# Linux counterpart of smoke.ps1, small on purpose: starts the built app on a virtual X display (software Vulkan) and
# checks two things a person does:
#
#  1. a PDF is opened, drawn on with the mouse (desktop mode: the left button is a stylus) and saved with Ctrl+S: the file
#     comes out with an Ink annotation of ours in it;
#  2. (The system's Save As and Open dialogs are the desktop portal's: a virtual display has none, so they are not driven here.)
#
# It proves the window, the GPU, the mouse and keyboard paths and the save on Linux; the rules themselves are
# the unit and stress tests' job.
#
#   tools/e2e/linux-smoke.sh [path/to/inkpdf] [path/to/inkpdf-gen]
#
#  3. dark look and dark pages (settings made up for the run): a pixel of the toolbar's panel and one of the page's paper are dark, where
#     in the light look they are white.
#
#  4. text: the text of a page is read behind the frame, and Ctrl+F finds a word in it (its places are lit on the page: yellow pixels, which a
#     page of black text on white has none of), and the select tool marks a drag over the text (blue pixels).
#
# Needs: xvfb (xvfb-run), xdotool, imagemagick (import, convert), mesa-vulkan-drivers (lavapipe), libxkbcommon-x11-0, a system font
# (fonts-liberation).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
target_dir="$(cargo metadata --format-version 1 --no-deps --manifest-path "$here/../../Cargo.toml" | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
target="$target_dir"
app="${1:-$target/debug/inkpdf}"
gen="${2:-$target/debug/inkpdf-gen}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/home/Documents" "$work/home/.config/InkPDF"
# (The program starts on the selection of text: these scenes draw ink, so they start on the pen.)
printf 'tool pen\n' >"$work/home/.config/InkPDF/settings.txt"
"$gen" "$work/t.pdf" 3 >/dev/null
cp "$work/t.pdf" "$work/original.pdf"

# The parts that run on the virtual display, as files of their own (xvfb-run runs a program, not a shell function).
cat >"$work/open.sh" <<DRIVE
set -eu
export HOME="$work/home"
unset XDG_CONFIG_HOME
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

# The dark look: settings made up for the run say dark with dark pages, and a screenshot is taken of the window with the page in it.
mkdir -p "$work/dark/.config/InkPDF"
printf 'appearance dark\ndark-pages on\n' >"$work/dark/.config/InkPDF/settings.txt"
cat >"$work/dark.sh" <<DRIVE
set -eu
export HOME="$work/dark"
unset XDG_CONFIG_HOME
"$app" "$work/t.pdf" --mode desktop --exit-after 10 2>"$work/log3.txt" &
pid=\$!
sleep 6
import -window root "$work/dark.png"
wait "\$pid"
DRIVE

# Search and select: the page's text is read by the indexer, found by Ctrl+F and selected by a drag.
mkdir -p "$work/text/.config/InkPDF"
printf 'tool select\n' >"$work/text/.config/InkPDF/settings.txt"
cat >"$work/text.sh" <<DRIVE
set -eu
export HOME="$work/text"
unset XDG_CONFIG_HOME
"$app" "$work/t.pdf" --mode desktop --exit-after 16 2>"$work/log4.txt" &
pid=\$!
sleep 6
win="\$(xdotool search --name 'page 1' | head -1)"
xdotool windowfocus "\$win"
sleep 0.3
xdotool mousemove 150 262
sleep 0.2
xdotool mousedown 1
for i in \$(seq 1 20); do
    xdotool mousemove \$((150 + i * 20)) \$((262 + i * 2))
    sleep 0.03
done
xdotool mouseup 1
sleep 1
import -window root "$work/select.png"
xdotool key ctrl+f
sleep 0.5
xdotool type --delay 80 'latency'
sleep 1.5
import -window root "$work/search.png"
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
xvfb-run -a -s "-screen 0 1280x960x24" bash "$work/dark.sh"
xvfb-run -a -s "-screen 0 1280x960x24" bash "$work/text.sh"

fail=0
check() { if eval "$2"; then echo "ok:   $1"; else echo "FAIL: $1"; fail=1; fi; }
check "the app started on a GPU" "grep -q '^GPU: ' '$work/log.txt'"
check "the document was saved (the file changed)" "! cmp -s '$work/t.pdf' '$work/original.pdf'"
check "the stroke is an Ink annotation" "grep -aq '/Subtype /Ink' '$work/t.pdf'"
check "it is one of ours (inkpdf- name)" "grep -aq 'inkpdf-' '$work/t.pdf'"
check "the input was recorded" "grep -q 'button pressed left' '$work/input.txt'"
check "and played back to the end" "grep -q '^opened r.pdf' '$work/play.err' && ! grep -qi panic '$work/play.err'"
# A pixel of the toolbar's panel (under the left pill's icons) and one of the page's margin, as red+green+blue.
sum_at() { convert "$work/dark.png" -crop "1x1+$1+$2" txt:- | tail -1 | grep -o 'srgb([0-9,]*)' | tr -dc '0-9,' | awk -F, '{print $1+$2+$3}'; }
check "the dark look draws the toolbar's panel dark" "test \"\$(sum_at 155 64)\" -lt 240"
check "and, with dark pages, the page's paper dark" "test \"\$(sum_at 60 300)\" -lt 240"
# Pixels of a colour in a screenshot (ImageMagick prints one line a pixel).
count_pixels() { convert "$work/$1" txt:- | grep -c "$2" || true; }
check "Ctrl+F lit the places a word was found, over the page (yellow pixels)" "test \"\$(count_pixels search.png 'srgb(255,23[0-9],1[3-6][0-9])')\" -gt 300"
check "a drag with the select tool lit the text it went over (blue pixels)" "test \"\$(count_pixels select.png 'srgb(2[0-1][0-9],2[2-3][0-9],255)')\" -gt 300"
exit "$fail"
