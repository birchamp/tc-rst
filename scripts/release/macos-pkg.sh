#!/usr/bin/env bash
# The macOS installer (card G3), ported from tC4 scripts/build-macos-pkg.zsh: an unsigned pkg
# whose payload is only the app, installed in /Applications, with the README shown first.
#
#   scripts/release/macos-pkg.sh <staged "translationCore 4.app"> <version> <output .pkg>
set -euo pipefail
app=$1 version=$2 output=$3
repo=$(cd "$(dirname "$0")/../.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/payload" "$work/instructions"
ditto "$app" "$work/payload/translationCore 4.app"
cp "$repo/scripts/release/README-macos.txt" "$work/instructions/README.txt"
pkgbuild --analyze --root "$work/payload" "$work/components.plist"
# Always replace the app where the pkg puts it, never a copy moved elsewhere.
plutil -replace 0.BundleIsRelocatable -bool false "$work/components.plist"
plutil -replace 0.BundleIsVersionChecked -bool false "$work/components.plist"
plutil -replace 0.BundleOverwriteAction -string upgrade "$work/components.plist"
pkgbuild --root "$work/payload" --component-plist "$work/components.plist" \
  --identifier org.unfoldingword.translationcore4 --version "${version%%-*}" \
  --install-location /Applications "$work/application.pkg"
productbuild --synthesize --package "$work/application.pkg" "$work/Distribution.xml"
node - "$work/Distribution.xml" <<'NODE'
const fs = require('fs');
const file = process.argv[2];
const xml = fs.readFileSync(file, 'utf8');
fs.writeFileSync(file, xml.replace('<installer-gui-script minSpecVersion="1">',
  '<installer-gui-script minSpecVersion="1">\n<title>translationCore 4</title>\n<readme file="README.txt" mime-type="text/plain"/>\n<domains enable_anywhere="false" enable_currentUserHome="false" enable_localSystem="true"/>'));
NODE
productbuild --distribution "$work/Distribution.xml" --resources "$work/instructions" \
  --package-path "$work" "$output"
echo "installer: $output"
