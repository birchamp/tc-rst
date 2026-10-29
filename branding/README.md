# App icons

tC4's icons, from `branding/` at `d25b7871` (#871): the translationCore mark, centered at 80% of a
transparent 1024 px square. They are committed so packaging needs no image tools.

| File | Used by |
|---|---|
| `icon-1024.png` | the window on Linux (`shell/lib.cjs` `windowOptions`) |
| `icon.ico` (16, 32, 48, 256 px) | the window on Windows; inside `electron.exe` (rcedit, `scripts/release/stage.sh`); the installer and shortcuts (`scripts/release/tc4.iss`) |
| `icon.icns` (16–1024 px) | the macOS app bundle's `electron.icns`: Finder, Launchpad and the Dock |

`scripts/release/smoke-installed.sh` fails an installed app whose icon is not these files.

To make them again on macOS, from the repository root, with `translationcore-logo.png` from
`crates/tc-ui/assets/images/`:

```sh
swift branding/make-icon.swift crates/tc-ui/assets/images/translationcore-logo.png branding/icon-1024.png
mkdir -p /tmp/tc4.iconset /tmp/tc4-ico
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" branding/icon-1024.png --out "/tmp/tc4.iconset/icon_${size}x${size}.png"
  sips -z "$((size * 2))" "$((size * 2))" branding/icon-1024.png --out "/tmp/tc4.iconset/icon_${size}x${size}@2x.png"
done
iconutil -c icns /tmp/tc4.iconset -o branding/icon.icns
for size in 16 32 48 256; do
  sips -z "$size" "$size" branding/icon-1024.png --out "/tmp/tc4-ico/$size.png"
done
node branding/make-ico.mjs /tmp/tc4-ico branding/icon.ico
```
