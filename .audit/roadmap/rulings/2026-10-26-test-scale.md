# Ruling: operation-count tests use a twice-larger corpus in the gate, ten times nightly

- Asked: 2026-10-25, by the owner: "If github is running out of memory shouldn't that tell us
  that are tests are excessive?"
- The evidence: on 2026-10-25, the gate's test job hit GitHub's 75-minute limit on batches 6 and
  7. tc-app's tests alone take about 39 minutes on this PC (peak 0.5 GB), and the operation-count
  binaries build Psalms and a corpus ten times larger for every row.
- Answer, in the owner's words: "we need to do what you've recommended for reduced tests".

## The rule

1. Operation counts are exact, so a cost that stays flat shows on any two sizes. The gate
   (`scripts/gates.sh` and CI) compares Psalms with the corpus about twice as large
   (`Spec::scaled(2)`).
2. The ten-times corpus runs in the nightly job. A nightly failure opens a card (CHK-7), as
   before.
3. One switch chooses the factor for every operation-count test, tc-journal's and tc-app's alike:
   2 by default, 10 when the nightly job sets it. A test that names "ten times larger" in its
   words names the factor it ran with instead.
4. The whole-Bible budget runs stay on cards P and G2, after the release.

## Effect for translators

None. The same rule (an action costs what it touches) is proved on every merge, and the big
corpus still runs every night.
