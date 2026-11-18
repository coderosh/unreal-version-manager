#!/bin/sh
# Builds the app, installs it to ~/.local/bin and adds it to the app menu.
set -e
cd "$(dirname "$0")"
cargo build --release
mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"
install -m755 target/release/unreal-version-manager "$HOME/.local/bin/unreal-version-manager"
cat > "$HOME/.local/share/applications/unreal-version-manager.desktop" <<DESK
[Desktop Entry]
Type=Application
Name=Unreal Version Manager
Comment=Install, launch and switch Unreal Engine versions
Exec=$HOME/.local/bin/unreal-version-manager
Icon=applications-games
Terminal=false
Categories=Development;
StartupWMClass=unreal-version-manager
DESK
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
echo "Installed. Launch \"Unreal Version Manager\" from your app menu."
