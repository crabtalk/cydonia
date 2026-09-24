#!/bin/sh
# Installs the latest cydonia release.
#
#   curl -fsSL https://cydonia.sh/install.sh | sh
#
# macOS (Apple silicon): the dmg's cydonia.app is copied into /Applications.
# Linux (x86_64, aarch64): the tarball is unpacked into ~/.local:
#   ~/.local/cydonia.app/                           the unpacked tarball
#   ~/.local/bin/cydonia                            symlink to its binary
#   ~/.local/share/applications/cydonia.desktop     launcher entry
#   ~/.local/share/icons/hicolor/*/apps/cydonia.png icons
set -eu

repo="https://github.com/crabtalk/cydonia"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fail() {
  echo "cydonia: $*" >&2
  exit 1
}

macos() {
  [ "$(uname -m)" = arm64 ] || fail "the macOS build is for Apple silicon only"
  # The dmg is named after its version; the latest release's tag says which.
  tag="$(curl -fsSLI -o /dev/null -w '%{url_effective}' "$repo/releases/latest")"
  version="${tag##*/v}"
  [ -n "$version" ] && [ "$version" != "$tag" ] || fail "could not find the latest release"

  url="$repo/releases/download/v$version/cydonia-$version-arm64.dmg"
  echo "downloading $url"
  curl -fL --progress-bar "$url" -o "$tmp/cydonia.dmg"

  hdiutil attach "$tmp/cydonia.dmg" -nobrowse -quiet -mountpoint "$tmp/mnt"
  rm -rf /Applications/cydonia.app
  ditto "$tmp/mnt/cydonia.app" /Applications/cydonia.app \
    || { hdiutil detach "$tmp/mnt" -quiet; fail "could not copy into /Applications"; }
  hdiutil detach "$tmp/mnt" -quiet

  echo "installed cydonia $version to /Applications/cydonia.app"
}

linux() {
  case "$(uname -m)" in
    x86_64 | amd64) arch=x86_64 ;;
    aarch64 | arm64) arch=aarch64 ;;
    *) fail "no build for $(uname -m)" ;;
  esac

  url="$repo/releases/latest/download/cydonia-linux-$arch.tar.gz"
  echo "downloading $url"
  curl -fL --progress-bar "$url" -o "$tmp/cydonia.tar.gz"
  tar -xzf "$tmp/cydonia.tar.gz" -C "$tmp"

  app="$HOME/.local/cydonia.app"
  rm -rf "$app"
  mkdir -p "$HOME/.local/bin" "$HOME/.local/share/applications"
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
}

case "$(uname -s)" in
  Darwin) macos ;;
  Linux) linux ;;
  *) fail "on Windows, run: irm https://cydonia.sh/install.ps1 | iex" ;;
esac
