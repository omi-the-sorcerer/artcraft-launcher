#!/bin/sh
# Compila e instala ArtCraft Launcher para el usuario actual:
# binario en ~/.local/bin, icono y acceso directo para rofi/ulauncher/menús.
set -e
cd "$(dirname "$0")"

cargo build --release

bin_dir="$HOME/.local/bin"
apps_dir="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
icon_dir="${XDG_DATA_HOME:-$HOME/.local/share}/icons/hicolor/scalable/apps"
mkdir -p "$bin_dir" "$apps_dir" "$icon_dir"

install -m 755 target/release/artcraft-launcher "$bin_dir/artcraft-launcher"
install -m 644 assets/artcraft-launcher.svg "$icon_dir/artcraft-launcher.svg"

cat > "$apps_dir/artcraft-launcher.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=ArtCraft Launcher
GenericName=Gestor de apps ArtCraft
Comment=Descarga, actualiza y lanza las Crafting Apps de ArtCraft
Exec=$bin_dir/artcraft-launcher
Icon=artcraft-launcher
Terminal=false
StartupWMClass=artcraft-launcher
Categories=Utility;
Keywords=artcraft;photocraft;filmcraft;vectorcraft;launcher;
DESKTOP

update-desktop-database "$apps_dir" 2>/dev/null || true

# Regenera los accesos directos de las apps ya instaladas.
"$bin_dir/artcraft-launcher" --sync-desktop

echo "Instalado: $bin_dir/artcraft-launcher"
case ":$PATH:" in *":$bin_dir:"*) ;; *) echo "Aviso: $bin_dir no está en PATH" ;; esac
