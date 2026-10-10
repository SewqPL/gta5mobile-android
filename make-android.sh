#!/bin/sh
set -eu
cd "$(dirname "$0")"
for tool in cargo rustup gradle java; do
    command -v "$tool" >/dev/null 2>&1 || { echo "Missing $tool. See android/README.md for setup." >&2; exit 1; }
done
if [ -z "${ANDROID_HOME:-}" ] && [ ! -f android/local.properties ]; then
    echo "Set ANDROID_HOME to your Android SDK, or configure android/local.properties." >&2
    exit 1
fi
rustup target add aarch64-linux-android
cargo ndk --version >/dev/null 2>&1 || { echo "Install cargo-ndk: cargo install cargo-ndk --locked" >&2; exit 1; }
gradle -p android assembleDebug
APK=android/app/build/outputs/apk/debug/app-debug.apk
echo "Built: $APK"
if [ "${1:-}" = --install ]; then adb install -r "$APK"; fi
