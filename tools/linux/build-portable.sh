#!/bin/bash
# A portable InkPDF for Linux (x86_64): one folder with the program and a portable.txt marker, so its settings are kept beside it
# (not in ~/.config), packed as InkPDF-Portable-<version>-linux-x86_64.tar.gz. Nothing is installed; unpack anywhere and run ./inkpdf.
# It links the system's libc, so it runs on a distribution about as new as the one that built it. It needs a Vulkan driver (Mesa's
# `mesa-vulkan-drivers`, or the GPU vendor's) and, on X11, libxkbcommon-x11; Wayland is reached through its libraries when present.
#   usage: tools/linux/build-portable.sh [output-folder]      (set CARGO_TARGET_DIR to build elsewhere than ./target)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
out="${1:-$root/dist}"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/apps/inkpdf/Cargo.toml" | head -1)"
cargo build --release -p inkpdf --bin inkpdf --manifest-path "$root/Cargo.toml"
exe="${CARGO_TARGET_DIR:-$root/target}/release/inkpdf"
name="InkPDF-Portable-$version-linux-x86_64"
mkdir -p "$out"
rm -rf "${out:?}/$name"
mkdir -p "$out/$name"
install -m 755 "$exe" "$out/$name/inkpdf"
printf '%s\n' "While this file is beside inkpdf, InkPDF keeps its settings in settings.txt here instead of in ~/.config/InkPDF. Delete this file to use the usual place." > "$out/$name/portable.txt"
cp "$root/apps/inkpdf/installer/readme.txt" "$out/$name/readme.txt"
cp "$root/LICENSE-MIT" "$root/LICENSE-APACHE" "$root/NOTICE" "$out/$name/"
cp "$root/apps/inkpdf/assets/fonts/LICENSE-LiberationFonts.txt" "$out/$name/"
tar -C "$out" -czf "$out/$name.tar.gz" "$name"
echo "built $out/$name.tar.gz ($(du -h "$out/$name.tar.gz" | cut -f1)); exe $(du -h "$out/$name/inkpdf" | cut -f1)"
