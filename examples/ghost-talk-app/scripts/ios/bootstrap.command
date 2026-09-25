#!/usr/bin/env bash
cd "$(dirname "$0")" || exit 1
./bootstrap.sh
rc=$?
printf "\nBootstrap exit code: %s\nPress Enter to close..." "$rc"
read -r _ || true
exit "$rc"
