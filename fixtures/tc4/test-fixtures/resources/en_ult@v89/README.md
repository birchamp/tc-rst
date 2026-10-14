# ULT v89 upgrade fixture

`metadata.json` and `TIT.usfm` are unchanged files from the real unfoldingWord
`en_ult` Scripture Burrito export at v89, cached by
`dev-env/scripts/cache-resource.zsh`. The metadata declares the full DCS revision,
ingredient byte size, checksum, and CC BY-SA 4.0 license.

The upgrade harness reduces the ingredient table to the included Titus file,
preserving that revision and its original checksum. It never fabricates a release
identity or substitutes v91 bytes for v89.

Source: <https://git.door43.org/unfoldingWord/en_ult/releases/tag/v89>.
