#!/usr/bin/env bash
# On a fresh runner (card G3): install the platform's installer as a translator would, start
# the installed app, run the server smoke test on the installed files, uninstall, and check the
# app data and projects are still there (tC4 #43, #44, #45).
#
#   scripts/release/smoke-installed.sh <platform> <dist dir>
set -euo pipefail
platform=$1 dist=$2
here=$(cd "$(dirname "$0")" && pwd)
np() { if command -v cygpath > /dev/null; then cygpath -w "$1"; else printf '%s' "$1"; fi; }
mkdir -p smoke-logs
manifest="$dist/BUILD-MANIFEST-$platform.json"

case "$platform" in
  linux-x64)
    install="$HOME/apps"
    mkdir -p "$install"
    unzip -q "$dist"/translationCore4-*-linux-x64.zip -d "$install"
    root="$install/translationCore 4"
    resources="$root/resources"
    start=(xvfb-run -a node "$here/app-starts.mjs" --exe "$root/translationCore4")
    appdata="$HOME/.config/translationCore4/app-data"
    uninstall() { rm -rf "$root"; }
    ;;
  windows-x64)
    installer=$(ls "$dist"/translationCore4-*-windows-x64*.exe)
    # Git Bash would turn /VERYSILENT into a path, and the installer would wait on its wizard.
    MSYS2_ARG_CONV_EXCL='*' "$installer" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /CURRENTUSER "/LOG=$(np "$PWD/smoke-logs/install.log")"
    root="$(cygpath -u "$LOCALAPPDATA")/Programs/translationCore 4"
    resources="$root/resources"
    start=(node "$(np "$here/app-starts.mjs")" --exe "$(np "$root/electron.exe")")
    appdata="$(cygpath -u "$APPDATA")/translationCore4/app-data"
    uninstall() { MSYS2_ARG_CONV_EXCL='*' "$root/unins000.exe" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART; # Inno's uninstaller runs a copy of itself and removes the folder last.
      for _ in $(seq 60); do [ -e "$root" ] || break; sleep 1; done; }
    ;;
  macos-arm64)
    sudo installer -pkg "$(ls "$dist"/translationCore4-*-macos-arm64*.pkg)" -target /
    root="/Applications/translationCore 4.app"
    resources="$root/Contents/Resources"
    start=(node "$here/app-starts.mjs" --exe "$root/Contents/MacOS/Electron")
    appdata="$HOME/Library/Application Support/translationCore4/app-data"
    uninstall() { sudo rm -rf "$root"; }
    ;;
  *) echo "smoke-installed.sh: no platform $platform" >&2; exit 2 ;;
esac

# #871: the translationCore mark, not Electron's, wherever the system shows the app's icon.
repo=$(cd "$here/../.." && pwd)
icon_is() { cmp -s "$1" "$repo/branding/$2" || { echo "FAIL the_app_icon_is_translationcore_s: $1 is not branding/$2"; exit 1; }; }
case "$platform" in
  linux-x64) icon_is "$resources/app/branding/icon-1024.png" icon-1024.png ;;
  windows-x64)
    icon_is "$root/icon.ico" icon.ico
    icon_is "$resources/app/branding/icon.ico" icon.ico
    # rcedit stores each of the .ico's images as an exe resource, byte for byte.
    node -e '
      const fs = require("fs");
      const [ico, exe] = process.argv.slice(1).map((f) => fs.readFileSync(f));
      for (let i = 0; i < ico.readUInt16LE(4); i++) {
        const at = 6 + 16 * i;
        const image = ico.subarray(ico.readUInt32LE(at + 12), ico.readUInt32LE(at + 12) + ico.readUInt32LE(at + 8));
        if (exe.indexOf(image) < 0) { console.log("FAIL the_app_icon_is_translationcore_s: electron.exe lacks image " + i); process.exit(1); }
      }
    ' "$(np "$repo/branding/icon.ico")" "$(np "$root/electron.exe")"
    ;;
  macos-arm64) icon_is "$resources/electron.icns" icon.icns ;;
esac
echo "ok the_app_icon_is_translationcore_s"

# The installed files are the ones the build recorded.
node -e '
  const [m, r] = process.argv.slice(1);
  const want = JSON.parse(require("fs").readFileSync(m, "utf8"));
  const got = JSON.parse(require("fs").readFileSync(r, "utf8"));
  if (want.commit !== got.commit || want.files.length !== got.files.length) { console.log("FAIL installed_manifest_is_the_build_s"); process.exit(1); }
  console.log("ok installed_manifest_is_the_build_s");
' "$(np "$manifest")" "$(np "$(dirname "$(find "$root" -name BUILD-MANIFEST.json | head -1)")/BUILD-MANIFEST.json")"

"${start[@]}" --app-data "$(np "$appdata")" 2>&1 | tee smoke-logs/app-starts.txt
exe=tc4; [ "$platform" = windows-x64 ] && exe=tc4.exe
node "$(np "$here/smoke.mjs")" --tc4 "$(np "$resources/tc4/$exe")" --ui "$(np "$resources/tc4/ui")" \
  --bundle "$(np "$resources/tc4/bundle")" --manifest "$(np "$manifest")" 2>&1 | tee smoke-logs/smoke.txt
cp "$appdata/shell.log" smoke-logs/ 2> /dev/null || true

uninstall
[ ! -e "$root" ] || { echo "FAIL uninstall_removes_the_app: $root is still there, holding:"; find "$root" | head -20; exit 1; }
echo "ok uninstall_removes_the_app"
[ -f "$appdata/app.lock" ] || [ -d "$appdata/resources" ] || { echo "FAIL uninstall_keeps_app_data: $appdata is gone"; exit 1; }
echo "ok uninstall_keeps_app_data"
[ -d "$HOME/translationCore4/projects" ] || { echo "FAIL uninstall_keeps_the_projects_folder"; exit 1; }
echo "ok uninstall_keeps_the_projects_folder"
made=$(sed -n 's/^projects folder: //p' smoke-logs/smoke.txt | tr -d '\r')
[ -n "$made" ] && [ -n "$(ls -A "$(cygpath -u "$made" 2> /dev/null || printf '%s' "$made")")" ] || { echo "FAIL uninstall_keeps_the_smoke_project: $made"; exit 1; }
echo "ok uninstall_keeps_the_smoke_project"
