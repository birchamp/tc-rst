# Ruling: one instance of translationCore per computer, in code and in tests

- Asked: 2026-10-22, by the owner, in conversation with the roadmap session.
- Answer, in the owner's words: "we want the code and the validation testing to enforce only one
  instance of TC4 on a computer at a time. So it shouldn't be testing for multiple sessions
  simultaneously, unless they're on different computers and merging via USB or door 43."

## The rule

1. **The code enforces one running instance per computer.** A second launch never writes a
   project (today: `tc4 serve`'s app-data lock answers `appLockHeld`, exit 3; G1's shell quits a
   second launch and focuses the first window).
2. **Tests never simulate two instances of the app writing at the same time on one computer,**
   and never two installations writing into one project folder at the same time. Those scenarios
   are outside REACH and are dropped, not logged as minors.
3. **Still in scope:**
   - one app session after another on the same computer (a kill or power cut, then a reopen);
   - another program (not translationCore) editing, holding or restoring a file the app wrote;
   - work from other computers arriving only through Door43 sync or a USB exchange file.
4. **Tests for rule 1** (a second launch is refused and writes nothing) stay and are required.

## Open point (the roadmap session's question to the owner)

S2's store has a "second writer" guard: if a journal file it did not write appears in the project
folder (for example a cloud-sync tool replicating another computer's writes into a folder the
app failed to recognise as synced), the project turns read-only with a notice. Under this ruling
that case is unreachable when synced folders are detected correctly. Keep the guard as a cheap
backstop with one test (recommended), or remove it. Until answered, the guard stays and only its
single "an unknown journal file appears, the project turns read-only" test is kept.

## Unblocks and changes

Card S2 (retire model steps and tests that run two writers on one folder at once), S3, G1, CHK-4.
