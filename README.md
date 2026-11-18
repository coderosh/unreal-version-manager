# Unreal Version Manager

A small native GUI (Rust + egui) for juggling Unreal Engine versions on Linux, where Epic ships no launcher.

## Features

- **Get versions**: opens Epic's Linux downloads page (you sign in there). When the download finishes, hit **⟳ Refresh**: any `Linux_Unreal_Engine_*.zip` in `~/Downloads` that isn't installed yet gets a one-click **Install** banner (also checked on startup).
- **Install from zip**: pick a `Linux_Unreal_Engine_*.zip`. It's extracted into the engines folder with a progress bar, a disk-space check, a cancel button, and Unix permissions and symlinks preserved.
- **Engines list**: scans the engines folder (default `~/UnrealEngine`) and reads each engine's version from `Engine/Build/Build.version`. You can also add an install that lives somewhere else.
- **Launch** any version, **register** it in `~/.config/Epic/UnrealEngine/Install.ini` (the same file UnrealVersionSelector uses), and **add or remove app-menu entries** for each version.
- **Launch through XWayland** (on by default): starts the editor with `SDL_VIDEODRIVER=x11`, which is more stable than native Wayland. Optionally **delete the zip after install**.
- **Delete** a version from disk. It asks for confirmation first, and also unregisters the engine and removes its menu entry.
- **Projects**: add `.uproject` files, or scan a folder for them (everything under `~/UnrealProjects` is picked up automatically). Choose an engine per project, which rewrites `EngineAssociation` to that engine's GUID, then open the project with that engine.

## Build / install

```sh
./install.sh          # cargo build --release + ~/.local/bin + app-menu entry
cargo run --release   # or just run it
cargo test            # extraction / registry / .uproject round-trip test
```

Settings live in `~/.config/unreal-version-manager/config.json`.

## Arch Linux package

`packaging/arch/PKGBUILD` builds from the `v$pkgver` tag on GitHub (`coderosh/unreal-version-manager`). It installs the binary, a menu entry and an icon.

```sh
cd packaging/arch
updpkgsums        # once the v0.1.0 tag exists, replaces the SKIP checksum
makepkg -si
```
