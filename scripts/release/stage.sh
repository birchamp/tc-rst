#!/usr/bin/env bash
# Lays out the installed app for one platform (card G3): Electronite, the shell, tc4, the UI
# and the bundle, the licence, THIRD-PARTY-NOTICES.txt and BUILD-MANIFEST.json. Every installer and zip is made from
# this folder, and the smoke test runs what it holds.
#
#   scripts/release/stage.sh <platform> <tc4 binary> <ui dir> <bundle dir> <electronite zip> <out dir>
#
# <platform> is linux-x64, windows-x64 or macos-arm64. TC4_BUILD_COMMIT must be the commit tc4
# and the UI were built from. TC_HELP_DESK_TOKEN and TC_HELP_DESK_EMAIL, when both are set, become
# the shell's help-desk key (docs/contracts/SHELL.md item 11); without them the app cannot send
# feedback, and says so. On windows-x64, TC4_RCEDIT must name rcedit-x64.exe v2.0.0, which puts
# the translationCore icon into electron.exe (#871).
set -euo pipefail

ELECTRONITE=v37.1.0-graphite
platform=$1 tc4=$2 ui=$3 bundle=$4 zip=$5 out=$6
repo=$(cd "$(dirname "$0")/../.." && pwd)
# Node on Windows reads C:\ paths, not Git Bash's /c/ ones.
np() { if command -v cygpath > /dev/null; then cygpath -w "$1"; else printf '%s' "$1"; fi; }

case "$platform" in
  linux-x64) want=41218aa3cd79f3449cfc360384ad4ef6064fe871e4c40a38ae859f2ddd8f8540 ;;
  windows-x64) want=8146ca21fa090e12bca01a8af73d234d9a31d90f51da0da86bb0e1731c371b52 ;;
  macos-arm64) want=a3dde44e03a076bc778f952f7a7a5ed6d8e5037d46ea6c1ba2deb6a11df59488 ;;
  *) echo "stage.sh: no platform $platform" >&2; exit 2 ;;
esac
# Read from stdin: given a path with backslashes (Windows), sha256sum puts a backslash before the hash.
sha256() { if command -v sha256sum > /dev/null; then sha256sum < "$1" | cut -d' ' -f1; else shasum -a 256 < "$1" | cut -d' ' -f1; fi; }
got=$(sha256 "$zip")
[ "$got" = "$want" ] || { echo "stage.sh: $zip has sha256 $got, not the pinned $want" >&2; exit 1; }
if [ "$platform" = windows-x64 ]; then
  rcedit_want=3e7801db1a5edbec91b49a24a094aad776cb4515488ea5a4ca2289c400eade2a
  got=$(sha256 "${TC4_RCEDIT:?TC4_RCEDIT must name rcedit-x64.exe}")
  [ "$got" = "$rcedit_want" ] || { echo "stage.sh: $TC4_RCEDIT has sha256 $got, not the pinned $rcedit_want" >&2; exit 1; }
fi

rm -rf "$out"
mkdir -p "$out"
if [ "$platform" = macos-arm64 ]; then
  # ditto keeps the app bundle's symbolic links, which unzip may not.
  ditto -x -k "$zip" "$out"
  # H16 part 3: installed as "translationCore 4".
  mv "$out/Electron.app" "$out/translationCore 4.app"
  root="$out/translationCore 4.app"
  resources="$root/Contents/Resources"
  plist="$root/Contents/Info.plist"
  /usr/libexec/PlistBuddy -c 'Set :CFBundleName translationCore' -c 'Set :CFBundleDisplayName translationCore 4' \
    -c 'Set :CFBundleIdentifier org.unfoldingword.translationcore4' "$plist" 2> /dev/null \
    || /usr/libexec/PlistBuddy -c 'Add :CFBundleDisplayName string translationCore 4' "$plist"
  # The bundle's CFBundleIconFile is electron.icns: Finder, Launchpad and the Dock show it.
  cp "$repo/branding/icon.icns" "$resources/electron.icns"
else
  root="$out/translationCore 4"
  mkdir -p "$root"
  unzip -q "$zip" -d "$root"
  resources="$root/resources"
fi
if [ "$platform" = windows-x64 ]; then
  # Explorer and the taskbar take the icon inside the exe; the installer and shortcuts, icon.ico.
  cp "$repo/branding/icon.ico" "$root/icon.ico"
  "$TC4_RCEDIT" "$(np "$root/electron.exe")" --set-icon "$(np "$root/icon.ico")"
fi

app="$resources/app"
mkdir -p "$app/shell" "$app/branding" "$resources/tc4"
cp "$repo"/shell/*.cjs "$app/shell/"
cp "$repo/branding/icon.ico" "$repo/branding/icon-1024.png" "$app/branding/"
cp "$repo/scripts/release/launch.cjs" "$app/launch.cjs"
version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' "$repo/Cargo.toml")
printf '{\n  "name": "translationCore4",\n  "productName": "translationCore4",\n  "version": "%s",\n  "private": true,\n  "main": "launch.cjs"\n}\n' \
  "$version" > "$app/package.json"
if [ -n "${TC_HELP_DESK_TOKEN:-}" ] && [ -n "${TC_HELP_DESK_EMAIL:-}" ]; then
  node -e 'process.stdout.write(JSON.stringify({ token: process.env.TC_HELP_DESK_TOKEN, email: process.env.TC_HELP_DESK_EMAIL }))' \
    > "$app/shell/help-desk.key"
fi

case "$platform" in windows-x64) exe=tc4.exe ;; *) exe=tc4 ;; esac
cp "$tc4" "$resources/tc4/$exe"
chmod +x "$resources/tc4/$exe"
cp -R "$ui" "$resources/tc4/ui"
cp -R "$bundle" "$resources/tc4/bundle"

docs="$root"
[ "$platform" = macos-arm64 ] && docs="$resources"
# Electronite's licence is where the app's LICENSE goes, so the notices take it first. On macOS
# it and Chromium's licences sit beside the app bundle, which is all that ships.
electronite_license="$root/LICENSE"
if [ "$platform" = macos-arm64 ]; then
  electronite_license="$out/LICENSE"
  mv "$out/LICENSES.chromium.html" "$resources/"
fi
node "$(np "$repo/scripts/release/notices.mjs")" --bundle "$(np "$bundle")" --electronite-license "$(np "$electronite_license")" \
  --out "$(np "$docs/THIRD-PARTY-NOTICES.txt")"
cp "$repo/LICENSE" "$docs/LICENSE"

if [ "$platform" = linux-x64 ]; then
  # tC4 docs/PACKAGING.md "The Chromium sandbox": after unzip chrome-sandbox is not setuid root,
  # and Electronite will not start without it unless told so.
  cat > "$root/translationCore4" <<'LAUNCH'
#!/bin/sh
here=$(cd "$(dirname "$0")" && pwd)
sandbox="$here/chrome-sandbox"
if [ -u "$sandbox" ] && [ "$(stat -c %u "$sandbox")" = 0 ]; then
  exec "$here/electron" "$@"
fi
echo "translationCore4: chrome-sandbox is not setuid root, so the window runs without Chromium's sandbox" >&2
exec "$here/electron" --no-sandbox "$@"
LAUNCH
  chmod +x "$root/translationCore4"
fi

if [ "$platform" = macos-arm64 ]; then
  # Apple silicon runs no unsigned code, and renaming the app and adding files broke
  # Electronite's signature: an ad-hoc signature, as tC4 rc.2 (unsigned, not notarized; H4).
  codesign --force --sign - "$resources/tc4/tc4"
  codesign --force --deep --sign - "$root"
  codesign --verify --deep --strict "$root"
fi

node "$(np "$repo/scripts/release/manifest.mjs")" --root "$(np "$root")" --into "$(np "$docs")" --platform "$platform" \
  --electronite "$ELECTRONITE" --electronite-sha256 "$want"
echo "staged $platform in $root"
