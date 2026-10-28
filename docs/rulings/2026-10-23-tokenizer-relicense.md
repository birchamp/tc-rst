# Licence: may unfoldingWord relicense string-punctuation-tokenizer to MIT?

- Asked: 2026-10-23, by the integration lane, through the orchestrator session, after the
  licence check (`rulings/2026-10-23-licence-mit.md`) found that `crates/tc-usfm/src/words/mod.rs`
  ports string-punctuation-tokenizer 2.0.0 (GPL-2.0, published by unfoldingWord).
- Options: yes, unfoldingWord relicenses it to MIT and the rebuild moves to MIT (recommended);
  no, the rebuild stays GPL-2.0-or-later.
- Answer: "1 yes. 2. build it. (this project will most likely replace original tC4)", 2026-10-23. Item 1: "yes".

## The rule

unfoldingWord, as the package's owner, relicenses string-punctuation-tokenizer to MIT, so the
rebuild's port of it may be MIT. The rebuild moves to MIT (card LIC-1). The relicensing of the
npm package itself is owner action A5 in `ROADMAP.md`, done in unfoldingWord's own repository,
not by this project's sessions unless the owner asks.

## Effect for translators

The About screen will say "License: MIT" and its licence dialog will show the MIT text, as
tC4's does since #838.

## Unblocks and changes

LIC-1 (new card). Clears the blocker in `rulings/2026-10-23-licence-mit.md`.
