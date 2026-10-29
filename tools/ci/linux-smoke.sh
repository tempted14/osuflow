#!/usr/bin/env bash
# Exercise our built package in a disposable CI display, never a player's game or desktop.
set -euo pipefail

if [[ "${1:-}" != "--inside" ]]; then
  script=$(realpath "$0")
  app_image=$(realpath "$1")
  report=$(pwd)/.runtime-smoke
  test_root=$(mktemp -d)
  mkdir -p "$report"
  cd "$test_root"
  "$app_image" --appimage-extract > "$report/extraction.log"
  exec xvfb-run -a -s "-screen 0 1280x900x24" dbus-run-session -- bash "$script" --inside "$app_image" "$test_root" "$report"
fi

app_image=$2
test_root=$3
report=$4
export XDG_CONFIG_HOME="$test_root/config" XDG_DATA_HOME="$test_root/data" XDG_CACHE_HOME="$test_root/cache" XDG_RUNTIME_DIR="$test_root/runtime"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME" "$XDG_CACHE_HOME" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
export APPIMAGE="$app_image" APPDIR="$test_root/squashfs-root" GDK_BACKEND=x11 LIBGL_ALWAYS_SOFTWARE=1

"$APPDIR/AppRun" --background > "$report/stdout.log" 2> "$report/stderr.log" &
test_pid=$!
trap 'kill "$test_pid" 2>/dev/null || true; wait "$test_pid" 2>/dev/null || true' EXIT
sleep 8
if ! kill -0 "$test_pid" 2>/dev/null; then
  cat "$report/stderr.log"
  echo "The packaged app exited before the startup check."
  exit 1
fi
xwininfo -root -tree > "$report/windows.log"
if ! grep -F '"osu!flow"' "$report/windows.log" > /dev/null; then
  cat "$report/stderr.log"
  echo "The first-run Settings window was not found."
  exit 1
fi
import -window root "$report/settings.png"
echo "Packaged Linux app stayed running and displayed its first-run Settings window."
