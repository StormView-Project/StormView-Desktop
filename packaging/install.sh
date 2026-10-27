#!/bin/sh
# Installs the release build for the current user (no root needed).
set -e
cd "$(dirname "$0")/.."
cargo build --release
install -Dm755 target/release/stormview "$HOME/.local/bin/stormview"
install -Dm644 app-icon.png "$HOME/.local/share/icons/hicolor/512x512/apps/stormview.png"
install -Dm644 packaging/stormview.desktop "$HOME/.local/share/applications/stormview.desktop"
echo "Installed. Launch StormView from your app menu, or run: stormview"
