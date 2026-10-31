#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Opens the GUI with a test texture for a few seconds and fails if Qt or QML
# reports a problem (a missing QML module or plugin, a QML error) or the app
# exits early.
#
#   packaging/smoke-test.sh COMMAND...      e.g. target/release/uvpad
#
# Set QT_QPA_PLATFORM=offscreen (or run under xvfb-run) where there is no display.
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
log=$(mktemp)
export QT_FORCE_STDERR_LOGGING=1

"$@" --gui "$root/tests/data/islands.png" > "$log" 2>&1 &
pid=$!
sleep "${SMOKE_SECONDS:-10}"
alive=yes
kill -0 "$pid" 2> /dev/null || alive=no
kill "$pid" 2> /dev/null
wait "$pid" 2> /dev/null
cat "$log"

if [[ $alive == no ]]; then
    echo "::error::uvpad exited during the smoke test"
    exit 1
fi
if grep -Eq 'qrc:/|is not installed|Could not (load|find)|Failed to (create|load)|Cannot load library' "$log"; then
    echo "::error::the smoke test log shows Qt/QML errors"
    exit 1
fi
echo "Smoke test passed"
