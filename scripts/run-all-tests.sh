#!/usr/bin/env bash
set -euo pipefail
SCRIPT="$(readlink -f "$0" 2>/dev/null || realpath "$0" 2>/dev/null || printf '%s' "$0")"
if [[ "${GHOST_TALK_IN_TERMINAL:-0}" != "1" && ! -t 1 ]]; then
  for terminal in x-terminal-emulator xfce4-terminal gnome-terminal mate-terminal konsole xterm; do
    command -v "$terminal" >/dev/null 2>&1 || continue
    case "$terminal" in
      xfce4-terminal) "$terminal" --command="env GHOST_TALK_IN_TERMINAL=1 bash '$SCRIPT'" ;;
      gnome-terminal|mate-terminal) "$terminal" -- env GHOST_TALK_IN_TERMINAL=1 bash "$SCRIPT" ;;
      *) "$terminal" -e env GHOST_TALK_IN_TERMINAL=1 bash "$SCRIPT" ;;
    esac
    exit 0
  done
fi
ROOT="$(cd "$(dirname "$0")/.." && pwd)"; cd "$ROOT"
PYTHON="$(command -v python3 || command -v python || true)"
[[ -n "$PYTHON" ]] || { echo "ERROR: Python 3.8+ is required." >&2; exit 1; }
set +e
"$PYTHON" qa/run-all-tests.py "$@"
RC=$?
set -e
printf '\n'
if [[ $RC -eq 0 ]]; then echo "ALL TESTS PASSED."; else echo "TEST SUITE FAILED with exit code $RC."; fi
if [[ "${GHOST_TALK_NO_PAUSE:-0}" != "1" && -t 0 ]]; then read -r -p "Press Enter to close..." _ || true; fi
exit "$RC"
