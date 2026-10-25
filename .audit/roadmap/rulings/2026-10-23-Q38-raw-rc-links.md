# Q38: raw rc:// links in helps

- Asked: 2026-10-23, by the integration lane (SWEEP), through the orchestrator session;
  `docs/TC4-DELTA-2026-10-23.md` §5.
- Options: (a) match tC4: show the raw text; (b) show the linked article's title as plain text
  (recommended); (c) make it a working link.
- Answer: "yes, needs-review, raw links are fine, yes orig lang highlights. MIT license new as of https://github.com/unfoldingWord/translationCore4/pull/838", 2026-10-23. Read as (a): "raw links are fine".

## The rule

A bare `rc://…` link or a `[[rc://…]]` link in a note or article shows as its raw text, as
tC4 4.0 shows it.

## Effect for translators

Some notes (for example BSOJ's Arabic notes for Titus) show an English address in the text.
Nothing is lost; the rebuild matches tC4.

## Unblocks and changes

The #628 (rc links) row of `docs/TC4-ISSUES.md` becomes a regression test to keep the raw
text (tc-helps, to create).
