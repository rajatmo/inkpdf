#!/usr/bin/env bash
# One-time setup of an Ubuntu machine (or Ubuntu on WSL) to build, test and package InkPDF for Linux, and with --android for Android
# too. On a Windows PC, Android builds natively with Android Studio's own tools (tools/android/build-apk.ps1), so WSL needs only the
# Linux part (see tools/cross-build.ps1). Run it as yourself; it asks for sudo only for apt.
#   bash tools/linux/setup-dev.sh [--android]
# Installs: the Linux build and test packages (the same as CI's `linux` job, plus a C toolchain), Rust (rustup, in ~/.cargo); with
# --android also a JDK, the Rust Android targets, the Android NDK and the SDK's build-tools and platform (in ~/android, about 2 GB more).
# Writes ~/inkpdf-dev.env, which the build scripts read. Safe to run again: what is there is kept.
set -euo pipefail
ndk_version=r27c
build_tools=36.0.0
platform=android-36
tools_zip=commandlinetools-linux-11076708_latest.zip
android="$HOME/android"
with_android=no
[ "${1:-}" = --android ] && with_android=yes

echo "== apt packages (sudo)"
sudo apt-get update
packages=(build-essential pkg-config curl ca-certificates git zip unzip python3 mesa-vulkan-drivers libxkbcommon-x11-0 libxcb-xinput0
    xvfb xdotool imagemagick fonts-liberation)
[ "$with_android" = yes ] && packages+=(openjdk-17-jdk-headless)
sudo apt-get install -y --no-install-recommends "${packages[@]}"

echo "== Rust"
if ! command -v rustup >/dev/null && [ ! -x "$HOME/.cargo/bin/rustup" ]; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --component clippy,rustfmt
fi
# shellcheck disable=SC1091
. "$HOME/.cargo/env"

env_file="$HOME/inkpdf-dev.env"
if [ "$with_android" = no ]; then
    cat > "$env_file" <<EOF
# Written by tools/linux/setup-dev.sh: builds go inside Linux, not to the Windows drive (faster, and the Windows build stays Windows-only).
. "\$HOME/.cargo/env"
export CARGO_TARGET_DIR="\$HOME/inkpdf-build-linux"
EOF
    echo "== done (Linux): $env_file"
    exit 0
fi

rustup target add aarch64-linux-android x86_64-linux-android
mkdir -p "$android"
echo "== Android NDK $ndk_version"
if [ ! -d "$android/android-ndk-$ndk_version" ]; then
    curl -fL -o "$android/ndk.zip" "https://dl.google.com/android/repository/android-ndk-$ndk_version-linux.zip"
    unzip -q "$android/ndk.zip" -d "$android" && rm "$android/ndk.zip"
fi

echo "== Android SDK: build-tools $build_tools, $platform"
sdkmanager="$android/sdk/cmdline-tools/latest/bin/sdkmanager"
if [ ! -x "$sdkmanager" ]; then
    curl -fL -o "$android/tools.zip" "https://dl.google.com/android/repository/$tools_zip"
    mkdir -p "$android/sdk/cmdline-tools"
    unzip -q "$android/tools.zip" -d "$android/sdk/cmdline-tools" && rm "$android/tools.zip"
    mv "$android/sdk/cmdline-tools/cmdline-tools" "$android/sdk/cmdline-tools/latest"
fi
yes | "$sdkmanager" --sdk_root="$android/sdk" --licenses >/dev/null || true
"$sdkmanager" --sdk_root="$android/sdk" "build-tools;$build_tools" "platforms;$platform"

cat > "$env_file" <<EOF
# Written by tools/linux/setup-dev.sh: where the Android tools are, and where Linux and Android builds go (inside Linux, not on the
# Windows drive: faster, and the Windows build directory stays Windows-only).
. "\$HOME/.cargo/env"
export ANDROID_HOME="$android/sdk"
export ANDROID_NDK_HOME="$android/android-ndk-$ndk_version"
export ANDROID_BUILD_TOOLS="$android/sdk/build-tools/$build_tools"
export ANDROID_JAR="$android/sdk/platforms/$platform/android.jar"
export CARGO_TARGET_DIR="\$HOME/inkpdf-build-linux"
EOF
echo "== done (Linux and Android): $env_file"
cat "$env_file"
