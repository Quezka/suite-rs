#!/usr/bin/env bash
# Builds KitDemo.apk (arm64) with the SDK's own tools: the kit's glass on a phone, for measuring frame times.
# Signed with a throwaway key in ~/.local/share/quezka-kitdemo (made on first run; never committed).
set -euo pipefail
SDK="${ANDROID_HOME:-$HOME/Android/Sdk}"
NDK="$SDK/ndk/27.2.12479018"
TOOLS="$SDK/build-tools/35.0.0"
JAR="$SDK/platforms/android-35/android.jar"
MIN_SDK=26
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
OUT="$HERE/build"
rm -rf "$OUT" && mkdir -p "$OUT/apk/lib/arm64-v8a"
CLANG="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CLANG/aarch64-linux-android$MIN_SDK-clang"
export CC_aarch64_linux_android="$CLANG/aarch64-linux-android$MIN_SDK-clang"
export AR_aarch64_linux_android="$CLANG/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"
(cd "$ROOT" && cargo build --release -p kit-demo-android --lib --target aarch64-linux-android)
cp "$ROOT/target/aarch64-linux-android/release/libkitdemo.so" "$OUT/apk/lib/arm64-v8a/"
"$TOOLS/aapt2" link -I "$JAR" --manifest "$HERE/AndroidManifest.xml" --min-sdk-version $MIN_SDK --target-sdk-version 34 \
    --version-code 1 --version-name 0.1.0 -o "$OUT/unsigned.apk"
(cd "$OUT/apk" && zip -q -0 -r "$OUT/unsigned.apk" lib)
"$TOOLS/zipalign" -f -p 16384 "$OUT/unsigned.apk" "$OUT/aligned.apk"
KEY="$HOME/.local/share/quezka-kitdemo/debug.jks"
if [ ! -f "$KEY" ]; then
    mkdir -p "$(dirname "$KEY")"
    keytool -genkeypair -keystore "$KEY" -storepass kitdemo -keypass kitdemo -alias kitdemo -keyalg RSA -keysize 2048 \
        -validity 10000 -dname "CN=Kit demo" >/dev/null 2>&1
fi
"$TOOLS/apksigner" sign --ks "$KEY" --ks-pass pass:kitdemo --key-pass pass:kitdemo --out "$HERE/KitDemo.apk" "$OUT/aligned.apk"
echo "$HERE/KitDemo.apk"
