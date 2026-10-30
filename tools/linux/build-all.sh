#!/usr/bin/env bash
# The Linux and Android half of a build of all three platforms (tools/cross-build.ps1 runs it in WSL; it also runs on any Ubuntu that
# tools/linux/setup-dev.sh has prepared). Tests and builds what is asked, and puts the packages in `out`.
#   bash tools/linux/build-all.sh [linux|android|all] [out-folder]
#   linux:   the unit tests, clippy, and the portable tarball (InkPDF-Portable-<version>-linux-x86_64.tar.gz)
#   android: an APK for phones and tablets and for the PC emulator (arm64-v8a and x86_64), signed with the test key (needs
#            setup-dev.sh --android; on Windows use tools/android/build-apk.ps1 instead)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
what="${1:-all}"
out="${2:-$HOME/inkpdf-dist}"
[ -f "$HOME/inkpdf-dev.env" ] || { echo "run tools/linux/setup-dev.sh first (no ~/inkpdf-dev.env)"; exit 1; }
# shellcheck disable=SC1091
. "$HOME/inkpdf-dev.env"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/apps/inkpdf/Cargo.toml" | head -1)"
mkdir -p "$out"
cd "$root"

if [ "$what" = linux ] || [ "$what" = all ]; then
    echo "== Linux: tests"
    cargo test -p inkpdf --lib -p inkpdf-api -p inkpdf-ext
    echo "== Linux: clippy"
    cargo clippy -p inkpdf -p inkpdf-api -p inkpdf-ext --all-targets -- -D warnings
    echo "== Linux: portable package"
    tools/linux/build-portable.sh "$out"
fi

if [ "$what" = android ] || [ "$what" = all ]; then
    echo "== Android: APK"
    ABIS="arm64-v8a x86_64" tools/android/build-apk.sh release "$out/InkPDF-$version-android.apk"
fi
echo "== packages in $out"
ls -la "$out"
