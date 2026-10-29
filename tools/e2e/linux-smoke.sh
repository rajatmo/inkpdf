#!/usr/bin/env bash
# Linux counterpart of smoke.ps1, small on purpose: starts the built app on a virtual X display (software Vulkan),
# draws with the mouse (desktop mode: the left button is a stylus), saves with Ctrl+S, and checks that the PDF came out
# with an Ink annotation of ours in it. It proves the window, the GPU, the mouse path and the save on Linux; the rules
# themselves are the unit and stress tests' job.
#
#   tools/e2e/linux-smoke.sh [path/to/inkpdf] [path/to/inkpdf-gen]
#
# Needs: xvfb (xvfb-run), xdotool, mesa-vulkan-drivers (lavapipe), libxkbcommon-x11-0, a system font (fonts-liberation).
set -euo pipefail

target="${CARGO_TARGET_DIR:-$(dirname "$0")/../../../inkpdf-build}"
app="${1:-$target/debug/inkpdf}"
gen="${2:-$target/debug/inkpdf-gen}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

"$gen" "$work/t.pdf" 3 >/dev/null
cp "$work/t.pdf" "$work/original.pdf"

# The part that runs on the virtual display, as a file of its own (xvfb-run runs a program, not a shell function).
cat >"$work/drive.sh" <<DRIVE
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

xvfb-run -a -s "-screen 0 1280x1024x24" bash "$work/drive.sh"

fail=0
check() { if eval "$2"; then echo "ok:   $1"; else echo "FAIL: $1"; fail=1; fi; }
check "the app started on a GPU" "grep -q '^GPU: ' '$work/log.txt'"
check "the document was saved (the file changed)" "! cmp -s '$work/t.pdf' '$work/original.pdf'"
check "the stroke is an Ink annotation" "grep -aq '/Subtype /Ink' '$work/t.pdf'"
check "it is one of ours (inkpdf- name)" "grep -aq 'inkpdf-' '$work/t.pdf'"
exit "$fail"
