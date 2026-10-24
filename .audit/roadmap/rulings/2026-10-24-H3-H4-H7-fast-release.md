# Ruling: finish in days, not weeks (scope, speed, H3, H4, H7)

- Asked: 2026-10-24, by the owner, in conversation with the roadmap session, after an estimate of
  4 to 6 weeks to a release candidate. The owner: "that's WAY WAY too long. We need a plan that
  will finish in a few days."
- Answered the same day, as choices in the session.

## Scope of the first release

1. **Cut until after the first release:** importing old projects, and Aquifer helps (AQ1, H6).
   - **Kept from import:** tC3 projects, because "there are no live tC4 projects in the field".
     tC4 4.0 projects, outside USFM projects and outside Burritos wait.
   - **Kept: Update a book from USFM.** The outside-edit notice ("This book has been changed
     externally. Would you like to import it into this project?") keeps working as ruled on
     2026-10-21.
2. **Kept:** Door43 sync, share and open (X, E1a, E1b), USB exchange (E2), exports and printing
   (D2), checkpoints and outside edits (D1), and conflict choices (C7a, C7b).

## How we work

3. **Merge in batches.** Three or four finished cards are gated together on one CI run. If the
   batch fails, it is split.
4. **Cleanup cards wait until after the release:** S3 (simplify the store), DOM-3 (capability
   tokens), CHK-5 (nightly jobs), V (format freeze), and the weekly SWEEP. The first SWEEP, which is
   already done, still merges.
5. **Whole-Bible scale runs wait until after the release:** P and G2. Op-count tests stay in the
   gate. The risk, accepted: a slow spot may reach translators first.
6. **8 to 10 sessions at once,** on separate files.

## Owner questions answered

- **H3 (platforms):** Windows, macOS and Linux, as tC4 ships.
- **H4 (signing):** Yakob does the signing, as for tC4.
- **H7 (look reference):** match tC4 4.0 rc.2 now (card REF1).

## Effect for translators

The first release does everything a team needs to translate, check, align, sync and exchange
work. Teams with tC3 projects can bring them in. Aquifer helps, and importing tC4 4.0 or outside
projects, come in a later release.

## Unblocks and changes

- **Parked:** AQ1, S3, DOM-3, CHK-5, V, P and G2. They leave DONE's dependencies.
- **Re-scoped:** I and F1 shrink to tC3 import and Update a book.
- **X:** builds on the current store. Its steps 4 to 8 no longer wait for S3.
- **G3 and REF1:** start now.
