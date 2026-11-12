#!/bin/bash
# Double-click this file (or run it in Terminal) to open Ianus.
# macOS marks downloaded programs as "quarantined" and refuses to open them;
# this clears that mark for this folder only.
cd "$(dirname "$0")" || exit 1
xattr -dr com.apple.quarantine . 2>/dev/null
chmod +x ./ianus
ROOM="${1:-rooms/chase.room}"
exec ./ianus "$ROOM"
