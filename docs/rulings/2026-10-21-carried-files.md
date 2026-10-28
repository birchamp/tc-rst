# Ruling: carried files (audio and other files that travel with a project)

- Asked: 2026-10-21, by the roadmap session, in conversation with the owner.
- Answer, in the owner's words:
  - "Could carried files have a version and a d43 user but no change history? Would that be
    enough?"
  - "We'll still need merge conflict resolution for those files when we implement them with tC."
  - "Yes, record that."

## The rule

1. **Each version of a carried file records:** its identity (the content hash; a screen may show a
   simple number), **the version it replaced**, the Door43 user who made it, and when.
2. **No history of old bytes is kept in the project.** When a version replaces another and nobody
   else changed that file, the old bytes may go. Every version that was synced stays in the Door43
   repository's git history.
3. **Conflicts stay.** Two versions that replaced the same version are a conflict. Both files stay
   on disk until someone chooses, in the conflict review: **Keep this one** (the other is then
   removed) or **Keep both** (the second is saved under its own name, for example
   `jon_01-Juan.wav`; the first draft of this ruling wrote `jon_01 (Juan).wav`, which the path
   grammar forbids: see H22 part 2 and the storage delta §2.7).
4. **When:** "when we implement them with tC". Carried files are built when translationCore adds
   the feature that uses them; until then the format reserves the record.

## Effect for translators

A teammate's recording is never silently replaced by another's. Old recordings that nobody is
choosing between are not kept in the project; synced ones can be recovered from Door43.

## Unblocks and changes

`docs/STORAGE-DELTA-2026-10-21.md`, cards S2, S3, C7b, E2, X, V; it replaces the H8(c) question.
