#!/usr/bin/env bash
# The real-input checks on Linux: builds the app (release, into ~/inkpdf-build-linux unless CARGO_TARGET_DIR says otherwise) and runs the scenarios
# on a virtual X display, where the mouse and keyboard are real (xdotool) and the stylus and finger are handed over by the app's --inject file.
#
#   tools/e2e/linux.sh [--only pages ...]
#
# Needs: xvfb (xvfb-run), xdotool, imagemagick, mesa-vulkan-drivers, libxkbcommon-x11-0, fonts-liberation (see tools/linux/setup-dev.sh).
set -euo pipefail
unset WAYLAND_DISPLAY
here="$(cd "$(dirname "$0")" && pwd)"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/inkpdf-build-linux}"
(cd "$here/../.." && cargo build --release -p inkpdf --bins 2>&1 | tail -1)
export INKPDF_TARGET="$CARGO_TARGET_DIR/release"
exec xvfb-run -a -s "-screen 0 1280x960x24" python3 "$here/run.py" linux "$@"
