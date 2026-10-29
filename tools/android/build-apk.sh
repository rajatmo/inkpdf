#!/usr/bin/env bash
# Build a signed APK of InkPDF for Android (a test build: signed with the debug key in this folder). arm64 by default; ABIS="x86_64" or
# ABIS="arm64-v8a x86_64" for an emulator.
#
#   ANDROID_NDK_HOME=…/android-ndk-r27c ANDROID_BUILD_TOOLS=…/build-tools/34.0.0 ANDROID_JAR=…/platforms/android-34/android.jar \
#       tools/android/build-apk.sh [release|debug] [out.apk]
#
# Needs: the Rust target (rustup target add aarch64-linux-android), the NDK (r27 or newer), the SDK's build-tools (aapt2,
# zipalign, apksigner) and platform android.jar, a JDK (apksigner), and zip. On GitHub's Ubuntu runners the SDK and NDK are
# already installed (see .github/workflows/ci.yml).
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
profile="${1:-release}"
out="${2:-$root/inkpdf-$profile.apk}"
: "${ANDROID_NDK_HOME:?set ANDROID_NDK_HOME to the NDK folder}"
: "${ANDROID_BUILD_TOOLS:?set ANDROID_BUILD_TOOLS to the build-tools folder (…/build-tools/34.0.0)}"
: "${ANDROID_JAR:?set ANDROID_JAR to platforms/android-34/android.jar}"
api=26   # the oldest Android the build runs on (8.0)
# Which CPUs to build for: arm64 phones and tablets by default; ABIS="x86_64" for an emulator on a PC, or both.
abis="${ABIS:-arm64-v8a}"
llvm="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
version="$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/apps/inkpdf/Cargo.toml" | head -1)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

flags=()
[ "$profile" = release ] && flags+=(--release)
[ "$profile" = release ] && export CARGO_PROFILE_RELEASE_STRIP=symbols
# Android 15 devices may use 16 KB pages: the library's segments are aligned for them.
export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-Wl,-z,max-page-size=16384"

for abi in $abis; do
    case "$abi" in
        arm64-v8a) target=aarch64-linux-android ;;
        x86_64) target=x86_64-linux-android ;;
        *) echo "unknown ABI $abi"; exit 1 ;;
    esac
    var="${target//-/_}"; up="$(echo "$var" | tr a-z A-Z)"
    # The NDK's compiler is the linker, and the C compiler for the few crates that have C in them.
    export "CC_$var=$llvm/${target}${api}-clang" "CXX_$var=$llvm/${target}${api}-clang++" "AR_$var=$llvm/llvm-ar"
    export "CARGO_TARGET_${up}_LINKER=$llvm/${target}${api}-clang"
    cargo build "${flags[@]}" -p inkpdf-android --target "$target" --manifest-path "$root/Cargo.toml"
    # (Where cargo put it: `.cargo/config.toml` builds beside the repository unless CARGO_TARGET_DIR says otherwise.)
    target_dir="$(cargo metadata --format-version 1 --no-deps --manifest-path "$root/Cargo.toml" | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')"
    so="$target_dir/$target/$profile/libinkpdf_android.so"
    [ -f "$so" ] || { echo "no $so"; exit 1; }
    mkdir -p "$work/lib/$abi"
    cp "$so" "$work/lib/$abi/"
done

# The package: manifest and resource table by aapt2, the library added to it, aligned, signed.
"$ANDROID_BUILD_TOOLS/aapt2" link --manifest "$here/AndroidManifest.xml" -I "$ANDROID_JAR" -o "$work/base.apk" \
    --min-sdk-version "$api" --target-sdk-version 34 --version-code 1 --version-name "$version-$profile"
(cd "$work" && zip -q -r base.apk lib)
"$ANDROID_BUILD_TOOLS/zipalign" -f -p 4 "$work/base.apk" "$work/aligned.apk"
[ -f "$here/debug.keystore" ] || keytool -genkeypair -keystore "$here/debug.keystore" -storepass android -keypass android \
    -alias androiddebugkey -keyalg RSA -keysize 2048 -validity 36500 -dname "CN=InkPDF test build,O=InkPDF,C=XX"
"$ANDROID_BUILD_TOOLS/apksigner" sign --ks "$here/debug.keystore" --ks-pass pass:android --key-pass pass:android \
    --ks-key-alias androiddebugkey --out "$out" "$work/aligned.apk"
"$ANDROID_BUILD_TOOLS/apksigner" verify --print-certs "$out" | head -3
ls -la "$out"
