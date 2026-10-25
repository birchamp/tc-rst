# Q39: highlight a note's words in the original-language pane (tC4 #840)

- Asked: 2026-10-23, by the integration lane (SWEEP), through the orchestrator session;
  `docs/TC4-DELTA-2026-10-23.md` §5. The first answer was "yes orig lang highlights"; asked again
  whether to build it now or wait for tC4.
- Options: (a) wait for tC4 and port it through REF1 (recommended then); (b) build it now from
  the issue's rule.
- Answer: "1 yes. 2. build it. (this project will most likely replace original tC4)", 2026-10-23. Item 2: "build it" (b).

## The rule

The rebuild builds it now: with a note current, the original-language pane marks the note's
quote words, matched against the verse's `\w` words with the note's occurrence; a quote that
does not match in full marks nothing; with no current note the pane stays plain. It applies to
the original tab of Draft and Understand and to the original row of the check card. The gateway
highlight is unchanged.

## Effect for translators

A checker sees the note's Greek or Hebrew words marked where the note is about them. This is a
declared difference from tC4 4.0 until tC4 ships #840.

## Unblocks and changes

OLH-1 (new card, app lane, M3).
