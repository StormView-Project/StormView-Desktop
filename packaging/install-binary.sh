#!/bin/sh
# Installs a downloaded release build for the current user (no root needed).
# Run from the extracted archive: ./install.sh
set -e
cd "$(dirname "$0")"
install -Dm755 stormview "$HOME/.local/bin/stormview"
install -Dm644 stormview.png "$HOME/.local/share/icons/hicolor/512x512/apps/stormview.png"
install -Dm644 stormview.desktop "$HOME/.local/share/applications/stormview.desktop"
echo "Installed. Launch StormView from your app menu, or run: stormview"
