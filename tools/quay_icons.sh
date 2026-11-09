#!/usr/bin/env bash
# Copies each suite app's own icon into Quay's bundle (crates/quay/assets/apps). Run from suite-rs, with the app
# repos cloned next to it. Stagno (ducknet) is taken from the installed copy when its repo isn't there.
set -euo pipefail
cd "$(dirname "$0")/.."
out=crates/quay/assets/apps
for app in quire gantry ligature folio bivouac facet missive natter quay; do
    cp "../$app/$app/assets/icon.svg" "$out/$app.svg"
done
cp ../ducknet/stagno/assets/icon.svg "$out/stagno.svg" 2>/dev/null \
    || cp /usr/share/icons/hicolor/scalable/apps/io.github.quezka.Stagno.svg "$out/stagno.svg"
echo "icons refreshed in $out"
