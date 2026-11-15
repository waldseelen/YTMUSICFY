#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"

mkdir -p "$BIN_DIR" "$APP_DIR"
ln -sf "${ROOT_DIR}/target/release/ytm-gui" "${BIN_DIR}/ytm-gui"
ln -sf "${ROOT_DIR}/target/release/ytm" "${BIN_DIR}/ytm"

cat > "${APP_DIR}/ytm-architect.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=YTM Architect
Comment=YouTube Music Playlist Studio (Sync, Dedupe, Sort, Diff)
Exec=${BIN_DIR}/ytm-gui
Icon=multimedia-player
Terminal=false
Categories=AudioVideo;Audio;Music;
StartupWMClass=ytm-architect
EOF

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true
fi

echo "Installed: ${BIN_DIR}/ytm-gui, ${BIN_DIR}/ytm, ${APP_DIR}/ytm-architect.desktop"
