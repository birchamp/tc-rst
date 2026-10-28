# Ruling: how much loss is acceptable

- Asked: 2026-10-22, offered by the owner unasked, to the S2 session (Claude Code add49644), while
  S2's proof found losses.
- Answer, in the owner's words: "I want to be sure that I haven't overstated something. Losing
  up to 2 minutes of a translator's work is acceptable. Losing a whole project or a project that
  is corrupted or can't be open is not."

## The rule

1. **Acceptable:** a crash, a power cut, or another program's change to a file the app wrote
   loses at most the translator's last 2 minutes of work.
2. **Never acceptable:** losing a whole project; a project that is corrupted (not a valid
   Scripture Burrito, or its records damaged so the app cannot use them); a project that cannot
   be opened; and losing work older than those 2 minutes.
3. Guarantees already built that keep more than this (no reported save lost at all) stay. The
   rule says what a test or a fix must prove, not what to remove.
4. The 2 minutes are measured back from the event (the crash, the power cut, the other
   program's change). (Inference by the S2 session; correct it if needed.)

## Effect for translators

After a crash or a power cut, a person may have to retype at most the last two minutes. Their
project always opens, is valid, and keeps everything older.

## Unblocks and changes

- Cards S2, S3, X, P, CHK-5: a long-model or mutation failure that loses only work saved in the
  last 2 minutes before the event that lost it is not a blocker; log it as a minor. A failure
  that loses older work, or leaves the project invalid or unopenable, still blocks.
- S2 on 2026-10-22: both product fixes of step 2 stay. Seed 20261021001 lost a save older than
  2 minutes (it had been through several checkpoints), and the adopted-book fix kept the
  Burrito valid.
- The model's oracle (`crates/tc-store/tests/store/model.rs`) still demands that no reported save
  is lost. Whether to relax it to this rule is S3's or CHK-5's choice; S2 does not change it.
