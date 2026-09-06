#!/usr/bin/env bash
set -euo pipefail

ghost_repo_root() { cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd; }

ghost_relaunch_in_terminal() {
  local script="$1"; shift || true
  if [[ "${GHOST_TALK_IN_TERMINAL:-0}" == "1" || -t 1 ]]; then return 0; fi
  local resolved
  resolved="$(readlink -f "$script" 2>/dev/null || realpath "$script" 2>/dev/null || printf '%s' "$script")"
  for terminal in x-terminal-emulator xfce4-terminal gnome-terminal mate-terminal konsole lxterminal xterm; do
    command -v "$terminal" >/dev/null 2>&1 || continue
    case "$terminal" in
      xfce4-terminal) "$terminal" --command="env GHOST_TALK_IN_TERMINAL=1 bash '$resolved'" ;;
      gnome-terminal|mate-terminal) "$terminal" -- env GHOST_TALK_IN_TERMINAL=1 bash "$resolved" ;;
      konsole|lxterminal|xterm|x-terminal-emulator) "$terminal" -e env GHOST_TALK_IN_TERMINAL=1 bash "$resolved" ;;
    esac
    exit 0
  done
}

ghost_pause() {
  if [[ "${GHOST_TALK_NO_PAUSE:-0}" != "1" && -t 0 ]]; then
    printf '\nPress Enter to close...'; read -r _ || true
  fi
}
