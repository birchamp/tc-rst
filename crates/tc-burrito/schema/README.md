# Scripture Burrito 1.0.0 schema bundle

`sb-1.0.0/` is a byte-for-byte copy of `fixtures/tc4/sb-schema/`, which translationCore 4
copied from pankosmia `resource-core` (tC4 commit
`d494a9d6ba9f4cdcc09faff9038c93159106045b`, `conformance/sb-schema/`). The test
`vendored_schema_matches_the_tc4_fixture` fails if the two copies drift apart.

`src/schema.rs` embeds the files with `include_str!`. The loader repairs three defects of
the bundle when it loads, so the files stay unedited:

- `scripture_flavor_type.schema.json` has a trailing comma, which strict JSON parsers
  refuse. The loader strips trailing commas from a file that does not parse.
- Six files are unreachable from `metadata.schema.json`, and one of them refers to
  schemas that do not exist. The loader registers only the files the root reaches.
- The files carry relative `$id` values (`./x.schema.json`, `.`, and
  `scripture/x.schema.json`, which resolves to the wrong folder). The loader sets each
  `$id` to `https://burrito.bible/schema/<path>`.

License: the Scripture Burrito schema is published under the MIT License,
Copyright (c) 2020-2026 Scripture Burrito contributors
(https://github.com/bible-technology/scripture-burrito).
