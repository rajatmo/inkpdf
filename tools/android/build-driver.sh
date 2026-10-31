#!/usr/bin/env bash
# Build the input driver of the real-input checks (driver/, a small instrumentation: not InkPDF, never shipped), signed with the debug key in this
# folder (the Linux twin of build-driver.ps1; keep the two in step).
#
#   ANDROID_BUILD_TOOLS=…/build-tools/34.0.0 ANDROID_JAR=…/platforms/android-34/android.jar tools/android/build-driver.sh [out.apk]
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
out="${1:-$root/../inkpdf-dist/InkPDF-input-driver.apk}"
: "${ANDROID_BUILD_TOOLS:?set ANDROID_BUILD_TOOLS to the build-tools folder (…/build-tools/34.0.0)}"
: "${ANDROID_JAR:?set ANDROID_JAR to platforms/android-34/android.jar}"
api=26
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/classes" "$(dirname "$out")"

javac --release 8 -Xlint:-options -cp "$ANDROID_JAR" -d "$work/classes" "$here/driver/app/inkpdf/driver/Drive.java"
"$ANDROID_BUILD_TOOLS/d8" --min-api "$api" --lib "$ANDROID_JAR" --output "$work" $(find "$work/classes" -name '*.class')
"$ANDROID_BUILD_TOOLS/aapt2" link --manifest "$here/driver/AndroidManifest.xml" -I "$ANDROID_JAR" -o "$work/base.apk" \
    --min-sdk-version "$api" --target-sdk-version 34 --version-code 1 --version-name 1
(cd "$work" && zip -q base.apk classes.dex)
"$ANDROID_BUILD_TOOLS/zipalign" -f -p 4 "$work/base.apk" "$work/aligned.apk"
"$ANDROID_BUILD_TOOLS/apksigner" sign --ks "$here/debug.keystore" --ks-pass pass:android --key-pass pass:android \
    --ks-key-alias androiddebugkey --out "$out" "$work/aligned.apk"
ls -la "$out"
