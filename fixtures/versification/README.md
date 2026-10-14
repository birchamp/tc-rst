# Versification fixtures

Tests in `crates/tc-versification` read these files. Do not edit them by hand.

## `vrs/proskomma/`

The six Paratext `.vrs` schemes (`eng`, `lxx`, `org`, `rsc`, `rso`, `vul`) bundled in
proskomma-core 0.11.3, as the published npm package serves them: `generate.mjs` writes
them from the GraphQL query `{ versifications { id vrs } }`. They are byte-identical to
`src/model/versifications/*_vrs.js` in https://github.com/Proskomma/proskomma-core at
commit `0204ad364fc2611cb4e7ab3b5cdeb083141d636b`.

Some lines end in spaces or tabs. That is part of the data and is kept.

## `vrs/proskomma-tests/`

`douay_rheims.vrs`, `rsc.vrs`, `truncated_versification.vrs` and `webbe.vrs`, copied on
2026-10-14 from `test/test_data/vrs/` of the same proskomma-core commit. They are
custom Paratext versification files that Proskomma's own tests use.

License of both directories: proskomma-core is MIT licensed, Copyright (c) 2020 Mark
Howe. The schemes derive from the standard versification files of Paratext (United
Bible Societies and SIL International).

## `oracle/`

Expected outputs for the Rust port, generated on 2026-10-14 by `oracle/generate.mjs`
(GPL-2.0-or-later, like the rest of this repository):

| file | produced by |
|---|---|
| `vrs2json.json` | proskomma-core 0.11.3 `vrs2json` on every `.vrs` file above |
| `hops.json` | proskomma-core 0.11.3 `mapVerse` through the forward and reversed table of 17 schemes: the six tC4 JSON schemes, the synthetic `syn` scheme and all ten `.vrs` files |
| `compose.json` | the same two-hop composition Proskomma's `do_cv` makes, for every pair of the six tC4 schemes and `syn` paired with `eng` and `org` |
| `tc4-map-reference.json` | tC4's own `src/data/mapReference.ts` (commit `d494a9d6ba9f4cdcc09faff9038c93159106045b`), run from its TypeScript source, for `eng` to and from each other scheme |

Each file states its input domain in its `note` field. To keep the files small, results
equal to their input are left out, and runs of single-verse shifts are written as one
line (`B c:v1..v2=B c:w`).

To regenerate (needs Node 22.18 or later and the npm registry):

```sh
ORACLE_DIR=$(mktemp -d)
(cd "$ORACLE_DIR" && npm init -y && npm i proskomma-core@0.11.3)
ORACLE_DIR="$ORACLE_DIR" TC4_DIR=<translationCore4 checkout> \
  node --no-warnings fixtures/versification/oracle/generate.mjs
```

The scheme JSON the oracle reads comes from `fixtures/tc4/test-fixtures/vrs/` (CC BY-SA
4.0, see `crates/tc-versification/schemes/README.md`).
