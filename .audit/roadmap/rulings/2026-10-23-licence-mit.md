# The rebuild's licence after tC4 moved to MIT

- Asked: 2026-10-23, by the integration lane (SWEEP), through the orchestrator session, as an
  owner note: tC4 is MIT since PR #838; the rebuild is GPL-2.0-or-later.
- Answer: "yes, needs-review, raw links are fine, yes orig lang highlights. MIT license new as of https://github.com/unfoldingWord/translationCore4/pull/838", 2026-10-23. The orchestrator reads it as: switch the rebuild to MIT.

## The rule

The rebuild moves to the MIT licence, matching tC4, once nothing in it is bound to a GPL-only
source.

## Effect for translators

The About screen will say "License: MIT", as tC4's does since #838 (D111).

## Unblocks and changes

Blocked on 2026-10-23: `crates/tc-usfm/src/words/mod.rs` reproduces string-punctuation-tokenizer
2.0.0 (GPL-2.0, published by unfoldingWord) rule for rule. The switch waits for the owner's
answer on relicensing that package. No Rust dependency is GPL-only; word-aligner (ISC),
word-aligner-lib (MIT), usfm-js (ISC) and xregexp (MIT) are fine. The switch also changes the
About screen in ten languages and its P7 captures.
