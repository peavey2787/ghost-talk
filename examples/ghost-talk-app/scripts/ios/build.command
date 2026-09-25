#!/usr/bin/env bash
cd "$(dirname "$0")" || exit 1
./build.sh
rc=$?
printf "\nBuild exit code: %s\nPress Enter to close..." "$rc"
read -r _ || true
exit "$rc"
