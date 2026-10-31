#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Opens the GUI with each test texture for a few seconds and fails if Qt or
# QML reports a problem (a missing QML module or plugin, a QML error, an image
# that does not decode) or the app exits early. tests/data/large.png is over
# Qt's default 256 MB decoding limit, so it checks that the limit is lifted.
#
#   packaging/smoke-test.sh COMMAND...      e.g. target/release/uvpad
#
# Set QT_QPA_PLATFORM=offscreen (or run under xvfb-run) where there is no display.
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
export QT_FORCE_STDERR_LOGGING=1
status=0

for image in islands.png large.png; do
    echo "--- $image"
    log=$(mktemp)
    "$@" --gui "$root/tests/data/$image" > "$log" 2>&1 &
    pid=$!
    sleep "${SMOKE_SECONDS:-12}"
    alive=yes
    kill -0 "$pid" 2> /dev/null || alive=no
    kill "$pid" 2> /dev/null
    wait "$pid" 2> /dev/null
    cat "$log"

    if [[ $alive == no ]]; then
        echo "::error::uvpad exited during the smoke test ($image)"
        status=1
    elif grep -Eq 'qrc:/|is not installed|Could not (load|find)|Failed to (create|load|initialize)|Cannot load library|Error decoding' "$log"; then
        echo "::error::the smoke test log shows Qt/QML errors ($image)"
        status=1
    fi
    rm -f "$log"
done

[[ $status == 0 ]] && echo "Smoke test passed"
exit "$status"
