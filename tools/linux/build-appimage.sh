#!/bin/bash
# An AppImage of InkPDF for Linux (x86_64): one file that runs on most distributions without installing anything (`chmod +x` it and run it, or
# double-click it). It packs the program of the portable build (tools/linux/build-portable.sh, which this runs first) with a launcher and a desktop
# entry, so that file managers and menus show the pen icon. It needs `appimagetool`, which is downloaded once to ~/.cache/inkpdf (set
# APPIMAGETOOL to use another), and FUSE 2 to run an AppImage (without it: `--appimage-extract-and-run`). Like the tarball it links the
# system's libc, so it runs on a distribution about as new as the one that built it, and needs a Vulkan driver (Mesa's `mesa-vulkan-drivers`).
#   usage: tools/linux/build-appimage.sh [output-folder]      (set CARGO_TARGET_DIR to build elsewhere than ./target)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
out="${1:-$root/dist}"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/apps/inkpdf/Cargo.toml" | head -1)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

"$here/build-portable.sh" "$work" >/dev/null
app="$work/InkPDF.AppDir"
mkdir -p "$app/usr/bin" "$app/usr/share/applications" "$app/usr/share/icons/hicolor/256x256/apps"
install -m 755 "$work/InkPDF-Portable-$version-linux-x86_64/inkpdf" "$app/usr/bin/inkpdf"
cp "$root/apps/inkpdf/installer/app-256.png" "$app/usr/share/icons/hicolor/256x256/apps/inkpdf.png"
cp "$root/apps/inkpdf/installer/app-256.png" "$app/inkpdf.png"
cat > "$app/inkpdf.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=InkPDF
Comment=Annotate PDFs with a pen
Exec=inkpdf %f
Icon=inkpdf
Categories=Office;Viewer;
MimeType=application/pdf;
Terminal=false
DESKTOP
cp "$app/inkpdf.desktop" "$app/usr/share/applications/inkpdf.desktop"
cat > "$app/AppRun" <<'RUN'
#!/bin/sh
here="$(dirname "$(readlink -f "$0")")"
exec "$here/usr/bin/inkpdf" "$@"
RUN
chmod +x "$app/AppRun"
cp "$root/LICENSE-MIT" "$root/LICENSE-APACHE" "$root/NOTICE" "$app/"

tool="${APPIMAGETOOL:-$HOME/.cache/inkpdf/appimagetool-x86_64.AppImage}"
if [ ! -x "$tool" ]; then
    mkdir -p "$(dirname "$tool")"
    curl -fL -o "$tool" "https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
    chmod +x "$tool"
fi
mkdir -p "$out"
file="$out/InkPDF-$version-x86_64.AppImage"
# (`--appimage-extract-and-run` so that the tool itself needs no FUSE, which a container or WSL may not have.)
ARCH=x86_64 "$tool" --appimage-extract-and-run "$app" "$file"
echo "built $file ($(du -h "$file" | cut -f1))"
