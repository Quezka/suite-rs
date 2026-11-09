#!/usr/bin/env bash
# Builds KitDemo.apk (arm64): the Rust library, then the Gradle shell around it (GameActivity, for the keyboard).
# Signed with a throwaway key in ~/.local/share/quezka-kitdemo (made on first run; never committed).
set -euo pipefail
SDK="${ANDROID_HOME:-$HOME/Android/Sdk}"
NDK="$SDK/ndk/27.2.12479018"
MIN_SDK=26
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../.." && pwd)"
CLANG="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin"
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CLANG/aarch64-linux-android$MIN_SDK-clang"
export CC_aarch64_linux_android="$CLANG/aarch64-linux-android$MIN_SDK-clang"
export CXX_aarch64_linux_android="$CLANG/aarch64-linux-android$MIN_SDK-clang++"
export AR_aarch64_linux_android="$CLANG/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS="-C link-arg=-Wl,-z,max-page-size=16384"
(cd "$ROOT" && cargo build --release -p kit-demo-android --lib --target aarch64-linux-android)
LIBS="$HERE/gradle-shell/app/src/main/jniLibs/arm64-v8a"
mkdir -p "$LIBS"
cp "$ROOT/target/aarch64-linux-android/release/libkitdemo.so" "$LIBS/"
# The C++ runtime GameActivity's glue needs.
cp "$NDK/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so" "$LIBS/"
KEY="$HOME/.local/share/quezka-kitdemo/debug.jks"
if [ ! -f "$KEY" ]; then
    mkdir -p "$(dirname "$KEY")"
    keytool -genkeypair -keystore "$KEY" -storepass kitdemo -keypass kitdemo -alias kitdemo -keyalg RSA -keysize 2048 \
        -validity 10000 -dname "CN=Kit demo" >/dev/null 2>&1
fi
(cd "$HERE/gradle-shell" && JAVA_HOME="${JAVA_HOME:-$HOME/.local/share/android-dev/jdk17}" ANDROID_HOME="$SDK" ./gradlew -q assembleRelease)
cp "$HERE/gradle-shell/app/build/outputs/apk/release/app-release.apk" "$HERE/KitDemo.apk"
echo "$HERE/KitDemo.apk"
