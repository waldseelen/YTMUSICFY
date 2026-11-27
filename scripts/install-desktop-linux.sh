#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICON_BASE="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor"
BIN_DIR="${HOME}/.local/bin"

mkdir -p "$APP_DIR" "$ICON_BASE/256x256/apps" "$ICON_BASE/scalable/apps" "$BIN_DIR"
rm -f "$APP_DIR/ytm-architect.desktop"

if command -v rsvg-convert >/dev/null 2>&1; then
    rsvg-convert -w 256 -h 256 "$REPO_ROOT/assets/brand/dalga-launcher.svg" -o "$ICON_BASE/256x256/apps/dalga.png"
else
    cp "$REPO_ROOT/assets/brand/dalga-icon-256.png" "$ICON_BASE/256x256/apps/dalga.png"
fi
cp "$REPO_ROOT/assets/brand/dalga-launcher.svg" "$ICON_BASE/scalable/apps/dalga.svg"
cp "$REPO_ROOT/assets/brand/dalga.desktop" "$APP_DIR/dalga.desktop"

if [[ -x "$REPO_ROOT/target/release/ytm-gui" ]] && ! [[ "$REPO_ROOT/target/release/ytm-gui" -ef "$BIN_DIR/ytm-gui" ]]; then
    cp "$REPO_ROOT/target/release/ytm-gui" "$BIN_DIR/ytm-gui"
fi
if [[ -x "$REPO_ROOT/target/release/ytm" ]] && ! [[ "$REPO_ROOT/target/release/ytm" -ef "$BIN_DIR/ytm" ]]; then
    cp "$REPO_ROOT/target/release/ytm" "$BIN_DIR/ytm"
fi

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APP_DIR" || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "$ICON_BASE" || true
fi

echo "Installed Dalga desktop entry, icons, and binaries."
