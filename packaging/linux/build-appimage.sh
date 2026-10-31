#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Packages an already built GUI binary as dist/uvpad-VERSION-linux-ARCH.AppImage.
#
#   packaging/linux/build-appimage.sh BINARY NOTICES
#
# Bundles the Qt that qmake6 (or $QMAKE) belongs to. An AppImage runs on
# distros at least as new as the one it was built on, so release builds are
# made on an old one (see .github/workflows/release.yml).
set -euo pipefail

binary=$(realpath "$1")
notices=$(realpath "$2")
root=$(cd "$(dirname "$0")/../.." && pwd)
arch=$(uname -m)
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -n 1)
id=io.github.yayameowmeow.uvpad
work=$root/target/appimage
appdir=$work/AppDir
tools=$work/tools

# linuxdeploy and its Qt plugin, which bundle the libraries the binary needs.
mkdir -p "$tools"
for tool in linuxdeploy/linuxdeploy linuxdeploy/linuxdeploy-plugin-qt; do
    file=${tool#*/}-$arch.AppImage
    if [[ ! -x $tools/$file ]]; then
        curl -fsSL -o "$tools/$file" \
            "https://github.com/linuxdeploy/${tool#*/}/releases/download/continuous/$file"
        chmod +x "$tools/$file"
    fi
done

rm -rf "$appdir"
install -Dm755 "$binary" "$appdir/usr/bin/uvpad"
install -Dm644 "$root/packaging/linux/$id.desktop" "$appdir/usr/share/applications/$id.desktop"
install -Dm644 "$root/assets/uvpad.png" "$appdir/usr/share/icons/hicolor/512x512/apps/$id.png"
install -Dm644 "$root/assets/uvpad-256.png" "$appdir/usr/share/icons/hicolor/256x256/apps/$id.png"
install -Dm644 "$root/LICENSE" "$appdir/usr/share/doc/uvpad/LICENSE"
install -Dm644 "$notices" "$appdir/usr/share/doc/uvpad/THIRD-PARTY-NOTICES.txt"

mkdir -p "$root/dist"
export PATH=$tools:$PATH
export QMAKE=${QMAKE:-$(command -v qmake6 || command -v qmake)}
plugins=$("$QMAKE" -query QT_INSTALL_PLUGINS)
# QML imports to bundle are found by scanning the app's QML.
export QML_SOURCES_PATHS=$root/qml
# X11 is always bundled; Wayland too, so it runs natively on Wayland desktops.
# Qt 6.10 merged the Wayland platform plugins into one libqwayland.so.
wayland=$(cd "$plugins/platforms" && ls libqwayland*.so | paste -sd ';' -)
export EXTRA_PLATFORM_PLUGINS=$wayland
# The xdg-desktop-portal theme, which gives the desktop's own file dialogs.
export DEPLOY_PLATFORM_THEMES=1
# No FUSE needed (CI containers, distros without libfuse2).
export APPIMAGE_EXTRACT_AND_RUN=1
export LDAI_OUTPUT=$root/dist/uvpad-$version-linux-$arch.AppImage
export OUTPUT=$LDAI_OUTPUT

"$tools/linuxdeploy-$arch.AppImage" \
    --appdir "$appdir" \
    --desktop-file "$appdir/usr/share/applications/$id.desktop" \
    --icon-file "$appdir/usr/share/icons/hicolor/256x256/apps/$id.png" \
    --plugin qt

# The Qt plugin does not reliably bundle Wayland's helper plugins: the shell
# integration (window management), decorations, and the buffer integrations,
# without which Qt cannot use OpenGL on Wayland and aborts. Add whichever are
# missing, then their libraries.
deps=()
for dir in wayland-shell-integration wayland-decoration-client wayland-graphics-integration-client; do
    if [[ ! -d $appdir/usr/plugins/$dir ]]; then
        cp -r "$plugins/$dir" "$appdir/usr/plugins/"
    fi
    deps+=(--deploy-deps-only "$appdir/usr/plugins/$dir")
done

"$tools/linuxdeploy-$arch.AppImage" \
    --appdir "$appdir" \
    "${deps[@]}" \
    --output appimage

echo "Built $LDAI_OUTPUT"
