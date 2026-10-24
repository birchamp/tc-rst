#!/usr/bin/env bash
# Makes the installer for one platform from the staged folder (card G3), beside a copy of its
# BUILD-MANIFEST.json named for the platform.
#
#   scripts/release/package.sh <platform> <stage dir> <dist dir>
#
# Windows: an Inno Setup installer, signed when SIGN=yes (sign-windows.ps1 as Inno's tc4sign).
# macOS: an unsigned pkg. Linux: a zip of the folder.
set -euo pipefail
platform=$1 stage=$2 dist=$3
repo=$(cd "$(dirname "$0")/../.." && pwd)
version=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' "$repo/Cargo.toml")
mkdir -p "$dist"
case "$platform" in
  linux-x64)
    cp "$stage/translationCore 4/BUILD-MANIFEST.json" "$dist/BUILD-MANIFEST-$platform.json"
    # -y keeps Electronite's symbolic links; unzip keeps the modes (tC4 docs/PACKAGING.md).
    (cd "$stage" && zip -qry "$OLDPWD/$dist/translationCore4-$version-linux-x64.zip" "translationCore 4")
    ;;
  windows-x64)
    cp "$stage/translationCore 4/BUILD-MANIFEST.json" "$dist/BUILD-MANIFEST-$platform.json"
    iscc="${TC4_ISCC:-/c/Program Files (x86)/Inno Setup 6/ISCC.exe}"
    [ -f "$iscc" ] || { echo "package.sh: Inno Setup 6 not found at $iscc" >&2; exit 1; }
    sign=()
    if [ "${SIGN:-}" = yes ]; then
      sign=(/DSign "/Stc4sign=powershell.exe -NoProfile -ExecutionPolicy Bypass -File \$q$(cygpath -w "$repo/scripts/release/sign-windows.ps1")\$q -ToolDir \$q$(cygpath -w "$RUNNER_TEMP/sign")\$q -Sign \$f")
    fi
    MSYS2_ARG_CONV_EXCL='*' "$iscc" "${sign[@]}" "/DPayload=$(cygpath -w "$stage/translationCore 4")" \
      "/DOutput=$(cygpath -w "$dist")" "/DVersion=$version" "$(cygpath -w "$repo/scripts/release/tc4.iss")"
    ;;
  macos-arm64)
    cp "$stage/translationCore 4.app/Contents/Resources/BUILD-MANIFEST.json" "$dist/BUILD-MANIFEST-$platform.json"
    "$repo/scripts/release/macos-pkg.sh" "$stage/translationCore 4.app" "$version" \
      "$dist/translationCore4-$version-macos-arm64-unsigned.pkg"
    ;;
  *) echo "package.sh: no platform $platform" >&2; exit 2 ;;
esac
ls -l "$dist"
