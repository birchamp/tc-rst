# HIST-1 lens: races and protocol (HEAD ca3ada86)

Summary as the lens subagent relayed it. No blockers and no majors.

Minors:
- m1: an aligner Restore ignores navigation made while it runs. Logged.
- m2: a mark clicked during a check's Restore can be written over by the Restore. Logged.
- m3: a dropped reply can say "Nothing was changed" when the Restore was saved. The wording is fixed; the rest is logged.
- m4: the aligner's History list can be stale while it is open, after an autosave or a sync. Logged with the loss lens's minor.
- m5: a doc comment sat on the wrong impl. Fixed.

Checked and fine:
- The check Restore token and the retry after LookAgain.
- An aligner Restore over a held aligner.
- Clicks during an aligner Restore.
- A save gate that fails or hangs.
- Double clicks.
- Request ids.
- Two tabs (LookAgain).
- The comment box.
- The Versions query key.
- last_sync behind a mutex, with syncs one at a time.
- The LastSync cache topic.
- A server restart (declared).
