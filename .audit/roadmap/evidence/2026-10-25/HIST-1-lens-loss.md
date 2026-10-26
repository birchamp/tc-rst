# HIST-1 lens: loss (HEAD 04f40a53)

Summary as relayed by the lens subagent. Blockers: none.

- M1: during Restore in the aligner, a click is saved over the restore. Fixed: the save gate runs first, and the screen shows Loading.
- M2: a comment save can land over a check's Restore. Fixed: the save gate runs first.
- M3: Received rows promise History for verse comments, but no screen showed it. Fixed: the comment box has History.
- M4: History rows did not say what differs. Fixed: rows show a check's status, bookmark and "no selection needed", and an alignment's pairs and marks.

Minors (logged in .audit/minors.tsv):
- The aligner's History token lags one autosave.
- Silent aligner Restore failures. Fixed.
- The text of a failed check Restore. Fixed.
- The file row wording. Fixed.
- The settings rows' older value. Fixed by ruling (a).

Checked and fine: two quick Restore clicks; Restore while a mark is in flight; refused saves; the store's restore is atomic; History of a moved check; History is hidden while in conflict; Received rows give one row per loser.
