# Ruling: edits made outside translationCore

- Asked: 2026-10-21, by the roadmap session, in conversation with the owner.
- Answer, in the owner's words, in order:
  - "If you want to edit the usfm (or md for OBS) elsewhere, then you can't expect the tc4
    project to work. However, I wouldn't want a project to break, just because a text file got
    changed."
  - "If the app can make a copy, and has an import mechanism, and a resolve conflicts workflow,
    why can't that be done internal to the app?"
  - "Door43 really has to merge. That's the point of all the users syncing to Door43."
  - On pushing a project folder to Door43 with another tool: "that's unsupported behavior."
  - The supported way to bring an edited USFM file to the team: "1. edit the usfm 2. import it
    into their tc4 project in tC4. 3. use tC4 to sync."
  - On the notice: "Could we say, 'This book has been changed externally. Would you like to
    import it into this project?'"
  - "Yes, record that."

## The rule

1. **The app's own record is the project.** The app writes each book file (USFM; Markdown for an
   OBS story) and every other generated file from its record. It never merges a file's changes
   into the record on its own.
2. **A project never breaks because a file changed.** It opens and works.
3. **A changed book file.** When the app opens a project, or its window comes back into focus,
   and a book file is not what the app wrote:
   1. it keeps the changed file inside the project first, so nothing is lost;
   2. it asks: "This book has been changed externally. Would you like to import it into this
      project?" with **Import** and **Not now**. Several books are named in one message;
   3. **Import** runs "Update a book from USFM" (or from Markdown for a story) on the kept copy:
      each changed verse becomes a change marked as imported, a verse that also changed in the
      project since goes to the conflict review, and then the book file is written from the
      project;
   4. **Not now** keeps the project's text and writes the book file from it. The kept copy stays
      in the conflict review's notices, with Import, until it is imported or dismissed.
4. **Other generated files** (`metadata.json`, settings files and the like) are written again from
   the record. A changed copy is kept inside the project's own store, with a one-line notice. No
   question is asked.
5. **"Update a book from USFM" is also an import in its own right**, for a file edited anywhere.
   It is the supported way to bring outside work in: edit, import, then sync.
6. **Door43 merges.** Teammates' work arrives by translationCore's sync and is merged, with any
   overlap shown as a conflict.
7. **Unsupported:** pushing a project folder to Door43 with any tool other than translationCore.
   If it happens, nothing is lost or broken: translationCore's next sync keeps its own text, the
   other version stays in Door43's git history, and the app says so once.

## Still open

Edits made with Door43's own tools (gatewayEdit, tcCreate, the web editor) on a team's project
repository: merged (today's design, STORAGE §11.2) or unsupported like rule 7. The roadmap
recommends unsupported. Owner question H18 in `ROADMAP.md`.

## Effect for translators

- Editing a book in Paratext and coming back gives one question and one click, never a broken or
  silently changed project.
- Nothing typed anywhere is lost: the outside version is always kept until imported or dismissed.

## Unblocks and changes

`docs/STORAGE-DELTA-2026-10-21.md` (the design changes), cards S2, S3, X, I, F1, D1, V; it replaces
the H8(a) question.
