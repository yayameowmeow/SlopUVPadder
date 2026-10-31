#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Packages GUI binaries as dist/uvpad-VERSION-macos-universal.dmg.
#
#   packaging/macos/build-app.sh NOTICES BINARY...
#
# Several binaries (one per architecture) are merged into a universal one.
# Bundles the Qt that qmake (or $QMAKE) belongs to; the official Qt builds are
# universal. The app is ad-hoc signed only, so macOS asks users to confirm the
# first launch (right-click > Open).
set -euo pipefail

notices=$1
shift
root=$(cd "$(dirname "$0")/../.." && pwd)
version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -n 1)
work=$root/target/macos
app=$work/uvpad.app
qmake=${QMAKE:-$(command -v qmake6 || command -v qmake)}
qt_bin=$("$qmake" -query QT_INSTALL_BINS)

rm -rf "$work"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
lipo -create "$@" -output "$app/Contents/MacOS/uvpad"
lipo -info "$app/Contents/MacOS/uvpad"
sed "s/@VERSION@/$version/g" "$root/packaging/macos/Info.plist" > "$app/Contents/Info.plist"
cp "$root/assets/uvpad.icns" "$app/Contents/Resources/uvpad.icns"
cp "$root/LICENSE" "$app/Contents/Resources/LICENSE"
cp "$notices" "$app/Contents/Resources/THIRD-PARTY-NOTICES.txt"

# Copies Qt's frameworks, plugins and the QML modules the app imports into the
# bundle, and signs everything (ad hoc).
"$qt_bin/macdeployqt" "$app" -qmldir="$root/qml" -codesign=- -always-overwrite

mkdir -p "$root/dist" "$work/dmg"
cp -R "$app" "$work/dmg/"
ln -s /Applications "$work/dmg/Applications"
cp "$root/LICENSE" "$notices" "$work/dmg/"
dmg=$root/dist/uvpad-$version-macos-universal.dmg
rm -f "$dmg"
hdiutil create -volname "uvpad $version" -srcfolder "$work/dmg" -ov -format UDZO "$dmg"
echo "Built $dmg"
