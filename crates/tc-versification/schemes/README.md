# Bundled versification schemes

These six files are compiled into the crate (`StandardScheme::json`). They are the
schemes tC4 offers at project creation, and the platform copies the chosen one, byte for
byte, into a project as `ingredients/vrs.json`.

| file | sha256 |
|---|---|
| `eng.json` | `5162023f8e0604fce96a49d7fb8cea73ef699928846d10d62210bd4882e81338` |
| `lxx.json` | `90d980f784512aea00af48646c337b1f7cb97bae3023a9223155369034e466f2` |
| `org.json` | `3b8e065fb592100265ff9ebcea71f5376927fa03c085844f3058d463a2e9cac7` |
| `rsc.json` | `c4a8ce2551d4f47bd56cafcf16c5b9169997937e2161e98d1af189a1b85a5d69` |
| `rso.json` | `93fbb63369c22022a932acbf562961f1f53998f1edf7140bf4ccb859ad0f3a95` |
| `vul.json` | `e09ea002e89aa86cdfbdd4cb69b1ff2c75070801e0648c37aef580d4fc8d06a0` |

## Provenance

- Copied from this repository's `fixtures/tc4/test-fixtures/vrs/`, which tC4 copied
  from `pankosmia/resource-core` `templates/content_templates/vrs/` (see
  `fixtures/tc4/test-fixtures/vrs/README.md`).
- On 2026-10-14 each file was checked byte-identical (same sha256) to
  `versification-mappings/standard-mappings/<name>.json` in the Copenhagen Alliance
  versification specification, https://github.com/Copenhagen-Alliance/versification-specification,
  branch `master`, HEAD `519af776ab65d89a51e37f750779250f940459ef`.
- The format was created by Mark Howe (United Bible Societies) from the Paratext `.vrs`
  format. The data derives from the Paratext standard versification files.

## License

The Copenhagen Alliance repository's `LICENSE.md` puts all non-code content, including
these data files, under the Creative Commons Attribution-ShareAlike 4.0 International
license (CC BY-SA 4.0), https://creativecommons.org/licenses/by-sa/4.0/. Attribution:
Copenhagen Alliance Versification Working Group. The files are unmodified.

## Known differences from Proskomma's bundled `.vrs` files

proskomma-core 0.11.3 ships its own copies of the same six schemes as Paratext `.vrs`
text (kept as test fixtures in `fixtures/versification/vrs/proskomma/`). The JSON files
here differ from them, so mapping through one or the other can differ:

- A `.vrs` file may list one source verse on several lines (`PSA 51:0 = PSA 51:1` and
  `PSA 51:0 = PSA 51:2` in `eng`). Proskomma's `vrs2json` keeps every target; the JSON
  keeps only the last one. This affects 4 entries in `eng`, 7 in `rsc`, 13 in `rso`, 51
  in `vul`, and the 12 `DAG` lines `lxx.vrs` lists twice.
- `lxx.json` lacks `1KI 20:1-29 = 1KI 21:1-29`; `rso.json` splits `PSA 115:0-10` into
  `PSA 115:0` and `PSA 115:1-10`; `vul.json` lacks `DAG 11:1-45 = DAN 11:1-45`.
- `vul.json` has no `maxVerses` for `EST`, `EZA` and `5EZ`. In `vul.vrs` those three lines
  end with a space, and the converter that produced the JSON dropped them. This is why
  tC4 reports "vul has no EST chapter 1".
- `vul.vrs` lists `6EZ 1:63 12:78`: chapter 12 has 78 verses, and chapters 2 to 11 are
  skipped, so `Versification::from_vrs` gives `6EZ` 12 chapters, the skipped ones with
  0 verses. `vul.json` has `"6EZ": ["63", "78"]`, so its chapter 2 has 78 verses and it
  has no chapter 12.
- `lxx.vrs` lists 304 excluded verses (`-GEN 31:51`, ...); `lxx.json` has none.
- `lxx.vrs` gives its 74 partial verses on `#!` lines (`#! *EXO 28:29,-,a `). Each of
  these lines ends with a space, which `lxx.json` keeps at the end of the last segment
  (`"a "`), and which a `.vrs` reader loses when it trims the line. For the same reason
  `Versification::to_vrs` refuses to write `lxx.json`.
- `eng.vrs` has one mapping that Paratext reads and Proskomma does not:
  `#! &ACT 19:40-41 = ACT 19:40`. `eng.json` does not have it.
