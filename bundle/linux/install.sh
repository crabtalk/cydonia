#!/bin/sh
# Installs the latest release into ~/.local:
#   ~/.local/cydonia.app/                           the unpacked tarball
#   ~/.local/bin/cydonia                            symlink to its binary
#   ~/.local/share/applications/cydonia.desktop     launcher entry
#   ~/.local/share/icons/hicolor/*/apps/cydonia.png icons
set -eu

case "$(uname -s)" in
  Linux) ;;
  *) echo "cydonia: this installer is for Linux" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch=x86_64 ;;
  aarch64 | arm64) arch=aarch64 ;;
  *) echo "cydonia: no build for $(uname -m)" >&2; exit 1 ;;
esac

url="https://github.com/crabtalk/cydonia/releases/latest/download/cydonia-linux-$arch.tar.gz"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "downloading $url"
curl -fL --progress-bar "$url" -o "$tmp/cydonia.tar.gz"
tar -xzf "$tmp/cydonia.tar.gz" -C "$tmp"

app="$HOME/.local/cydonia.app"
rm -rf "$app"
mkdir -p "$HOME/.local" "$HOME/.local/bin" "$HOME/.local/share/applications"
mv "$tmp/cydonia.app" "$app"
ln -sf "$app/bin/cydonia" "$HOME/.local/bin/cydonia"

cp -R "$app/share/icons" "$HOME/.local/share/"
sed "s|^Exec=cydonia|Exec=$app/bin/cydonia|" "$app/share/applications/cydonia.desktop" \
  > "$HOME/.local/share/applications/cydonia.desktop"
command -v update-desktop-database >/dev/null 2>&1 \
  && update-desktop-database "$HOME/.local/share/applications" || true

echo "installed cydonia to $app"
case ":$PATH:" in
  *":$HOME/.local/bin:"*) ;;
  *) echo "add ~/.local/bin to PATH to run cydonia from a shell" ;;
esac
