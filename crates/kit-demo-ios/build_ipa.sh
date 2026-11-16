#!/bin/bash
# unsigned ipa: macOS + Xcode + `rustup target add aarch64-apple-ios`
# sideload: AltStore | Sideloadly re-sign w/ user apple id (free = 7 days)
set -euo pipefail
cd "$(dirname "$0")/../.."
cargo build --release --target aarch64-apple-ios -p kit-demo-ios
out=target/ios
rm -rf "$out" && mkdir -p "$out/Payload/KitDemo.app"
cp target/aarch64-apple-ios/release/kitdemo "$out/Payload/KitDemo.app/"
cp crates/kit-demo-ios/ios/Info.plist "$out/Payload/KitDemo.app/"
if [ -f icons/apps/kit-demo-180.png ]; then cp icons/apps/kit-demo-180.png "$out/Payload/KitDemo.app/AppIcon60x60@3x.png"; fi
(cd "$out" && zip -qr KitDemo.ipa Payload)
echo "$out/KitDemo.ipa"
