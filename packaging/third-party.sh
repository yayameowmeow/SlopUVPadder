#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Writes the THIRD-PARTY-NOTICES.txt that ships with a release build.
#
#   packaging/third-party.sh OUTPUT FEATURES [QT_VERSION]
#
# FEATURES is the cargo feature list the build used ("" for the CLI, or e.g.
# "gui,avif"); QT_VERSION is the Qt the GUI build bundles. Needs cargo-about.
set -euo pipefail

out=$1
features=$2
qt_version=${3:-6}
root=$(cd "$(dirname "$0")/.." && pwd)
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT

(cd "$root" && cargo about generate --features "$features" packaging/about.hbs -o "$tmp")

{
    cat <<EOF
uvpad is free software, licensed under the GNU General Public License,
version 3 or later (see LICENSE). Its source code is available at
https://github.com/yayameowmeow/SlopUVPadder

This build also contains the third-party software listed below.

EOF
    if [[ ,$features, == *,gui,* ]]; then
        cat <<EOF
===============================================================================
Qt $qt_version
===============================================================================
Copyright (C) The Qt Company Ltd. and other contributors.

Qt is used under the GNU Lesser General Public License, version 3, whose text
follows. Qt is included as separate shared libraries, which you may replace
with your own builds. Qt's source code is available at
https://download.qt.io/official_releases/qt/ and the licenses of the
third-party code that Qt itself contains are listed at
https://doc.qt.io/qt-6/licenses-used-in-qt.html

EOF
        cat "$root/packaging/licenses/LGPL-3.0.txt"
        echo
    fi
    if [[ ,$features, == *,avif,* ]]; then
        cat <<EOF
===============================================================================
dav1d (AVIF decoding), statically linked
===============================================================================
EOF
        cat "$root/packaging/licenses/dav1d-BSD-2-Clause.txt"
        echo
    fi
    echo "==============================================================================="
    cat "$tmp"
} > "$out"
