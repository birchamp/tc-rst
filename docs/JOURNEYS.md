# User journeys

**Answers:** what a person does with the app, and what must be true when they finish. **Wins
over:** the design documents on what a person sees and what ends up on disk. Owner rulings win
over it, and a ruling updates the entry it touches in the same change. **Does not cover:** how
the app is built (`docs/APP-DESIGN.md`, `docs/STORAGE.md`) or whether a journey passes today
(run the proof).

This is the only list of journeys. J1 to J26 are tC4's (`docs/JOURNEYS.md` on branch
`legacy-4.0`, the same at the reference commit `d25b7871`), rewritten here for the rebuild. J27
and later are the rebuild's own; a new feature adds its journey here (`docs/PLAN.md` "How a new
feature enters").

## Rules

- A journey is a goal one actor reaches. It is done when its **end state** holds: what the person
  sees, and what is on disk. A screen state alone never counts.
- `Jn` never changes meaning. A retired number is never reused.
- Steps are in the person's words. Their order is not fixed unless they are numbered.
- Each journey serves one or more commitments of `docs/VISION.md` (V1 to V8).
- "Must not" lines are part of the end state. A journey that passes while breaking one fails.
- Where an entry says **Differs from tC4**, the app chose the translator's need over what tC4
  did. The full list of differences, with reasons, is `docs/DIFFERENCES.md`.
- **Proof.** The tests that prove each journey are listed under its id in
  `parity/app/journeys/coverage.json`: tC4's journeys under `journeys`, the rebuild's under
  `rebuildJourneys`. `parity/app/journeys/coverage.test.mjs` fails when a listed test is missing,
  when a journey has no entry, or when a pending card is done (INV-P1-COVERED). To find the proof
  of one journey: `grep -n '"J17"' parity/app/journeys/coverage.json`.
- Only reachable scenarios count: the app's own screens and API, the app or computer stopping at
  any moment, another program editing a project file, a teammate's work arriving by sync or file,
  a full disk, the network dropping. Nothing else is a journey or a bug.

Actors: **translator** (drafts and checks), **facilitator** (sets up projects, resources and
deliverables), **consultant** (reviews; Phase 2). Activities: Start, Understand, Translate,
Check, Exchange, Deliver.

## Index

| Id | Actor | Activity | Goal | Serves | Kind |
|---|---|---|---|---|---|
| J1 | facilitator | Start | Create a Bible project and add books | V6 | tC4 |
| J2 | translator | Translate | Draft verses beside the sources | V7, V1 | tC4 |
| J3 | facilitator | Start | Get pinned resources | V4, V2 | tC4 |
| J4 | translator | Check | Check a book with tN and tW | V7 | tC4 |
| J5 | translator | Check | Align a verse | V7 | tC4 |
| J6 | translator | Translate | Edit a checked verse and see the checks flagged | V1, V7 | tC4 |
| J7 | facilitator | Deliver | Export the book | V6, V5 | tC4 |
| J8 | translator | Translate | Resume work across sessions and books | V1 | tC4 |
| J9a | facilitator | Exchange | Import a tC3 project as a new project | V6, V1 | tC4 |
| J9c | facilitator | Exchange | Import raw USFM as a new project | V6, V1 | tC4 |
| J9d | facilitator | Exchange | Import a Scripture Burrito as a new project | V6, V1 | tC4 |
| J11 | facilitator | Exchange | Share the project to Door43 | V3 | tC4 |
| J12 | facilitator | Start | Upgrade the pinned resources | V4, V1 | tC4 |
| J13 | facilitator | Start | Change the gateway-language resource set | V4 | tC4 |
| J16 | translator | Understand | Read a passage with helps and write a user comment | V7 | tC4 |
| J17 | translator | Exchange | Open a shared project on another computer | V3 | tC4 |
| J18 | translator | Translate | Keep one version of a verse in conflict | V3, V1 | tC4 |
| J19 | consultant | Check | Record findings on a translation | V7 | Phase 2 |
| J20 | facilitator | Start | Create an OBS project | V6 | tC4 |
| J21 | translator | Translate | Translate a story frame by frame | V7 | tC4 |
| J22 | translator | Check | Check an OBS story | V7 | tC4 |
| J23 | facilitator | Deliver | Export an OBS project as Markdown and PDF | V6 | tC4 |
| J24 | facilitator | Exchange | Share an OBS project to Door43 | V3 | tC4 |
| J25 | translator | Understand | Read a story with helps and write a user comment | V7 | tC4 |
| J26 | translator | Exchange | Sync one project with two teammates through Door43 | V3, V1 | tC4 |
| J27 | translator | Exchange | Exchange a project by file, with no internet | V3, V2 | rebuild |
| J28 | translator | Exchange | See what a newer version replaced, and restore the older one | V1, V3 | rebuild |
| J29 | translator | Translate | Take in a book another program changed | V1 | rebuild |
| J30 | translator | Understand | Read helps from Aquifer | V4 | rebuild |

Retired: J9b (x-tcore import: no data exists), J10 (right to left is an axis of J2, J4, J5 and
J7, not a journey), J14 (project isolation is a "must not" of J1 and J2), J15 (a slow open is a
budget in `docs/PERFORMANCE.md`). Their tC4 tests stay mapped under `specs` in `coverage.json`.

## Entries

### J1 Create a Bible project

- Actor: facilitator. Activity: Start. Serves V6.
- Before: a fresh install. The first run and the interface language are steps here.
- Steps: start the app · choose the interface language · choose the target language and its
  direction · name the project · add one or more books.
- End state: Home shows the new project. On disk, one new project folder that is a valid
  Scripture Burrito, with one USFM book per added book and its own author id. A regional or
  private-use language code is kept whole; an invalid one is refused and leaves nothing.
- Must not: touch any other project. A second project is a second folder, and the first stays
  byte for byte the same.

### J2 Draft verses beside the sources

- Actor: translator. Activity: Translate. Serves V7, V1.
- Before: J1.
- Steps: open a book · read the source texts and the original-language pane · type a section
  straight through · place the verse numbers · stack two verse numbers into a span, or drag one
  past text to break it · leave the section · see progress move. Verse by verse stays available.
- End state: the section's verse texts are in the book with every verse number placed. Text
  outside the edited verses is byte for byte the same. When a span was made or broken, the
  alignments and decisions on those verses are marked invalid and kept. "Saved" shows only when
  the save is on disk.
- Must not: write to another project; save a section before Save; rewrite or drop an alignment or
  decision.
- Differs from tC4: a section draft not yet saved survives a reload; its card opens again with
  the typed text.

### J3 Get pinned resources

- Actor: facilitator. Activity: Start. Serves V4, V2.
- Steps: choose the gateway language · pick the resource set · download.
- End state: the project's pins name each resource's exact release; the resources are installed
  and match their pins; the helps open with the internet off; a missing resource shows that it is
  missing. Download is always offered. While the internet is off it asks "Turn on the internet?"
  first, and Not now sends nothing.
- Must not: install a file of another release than the pin; change the book text.

### J4 Check a book

- Actor: translator. Activity: Check. Serves V7.
- Steps: open the translationWords or translationNotes list · read a note or an article · mark
  the words that translate it · write a check comment · set a bookmark.
- End state: one decision record per check, holding the marked words, the comment and the
  bookmark; they survive reopening; clearing a comment clears that field. The book text is
  unchanged.
- Must not: store a check comment anywhere but its decision; count a bookmarked or commented check
  differently from a plain one.

### J5 Align a verse

- Actor: translator. Activity: Check. Serves V7.
- Steps: link and unlink word pairs · turn suggestions on · accept or reject each suggestion, or
  all of them.
- End state: the verse's alignment is saved; the text is unchanged. An accepted suggestion is the
  same as a link made by hand. A suggestion shown and not accepted leaves no trace.
- Must not: save an unconfirmed suggestion; count a verse as aligned while a suggestion stands;
  make the translator wait for training after a save.
- Differs from tC4: across a verse edit, each link moves with its own word.

### J6 Edit a checked verse

- Actor: translator. Activity: Translate. Serves V1, V7.
- Steps: edit a verse that has check decisions · see the affected checks flagged.
- End state: the new verse text is saved; the affected decisions are marked invalid and kept.
  Nothing is deleted.

### J7 Export the book

- Actor: facilitator. Activity: Deliver (inside Check). Serves V6, V5.
- Steps: preview the typeset text · choose an export · save it outside the project.
- End state: files outside the project: a dated PDF, aligned USFM, plain USFM and a Scripture
  Burrito, each named as tC4 names it. The project is byte for byte the same, except a
  checkpoint.
- Must not: write anything else into the project.
- Differs from tC4: aligned USFM weaves a verse's alignment only when the verse needs no review
  and its words match; any other verse keeps its stored text, and the export names it. An export
  of a book with verses in conflict asks first.

### J8 Resume work

- Actor: translator. Activity: Translate. Serves V1.
- Steps: leave Translate or the project · come back later · open another book.
- End state: leaving with changes since the last checkpoint makes exactly one checkpoint;
  leaving with none makes none; the last draft is what opens again. A quit within the two seconds after an
  aligner change saves the change first, then checkpoints.
- Differs from tC4: a mode switch checkpoints at most once every two minutes after one that
  committed.

### J9a, J9c, J9d Import as a new project

- Actor: facilitator. Activity: Exchange. Serves V6, V1.
- Sources: (a) a tC3 project · (c) raw USFM · (d) a Scripture Burrito.
- End state: one **new** project, a valid Scripture Burrito, whose text is byte for byte the
  source's text. The name on the review page is the project's name everywhere. Looking up resource
  versions on Door43 is an explicit step, and it asks before it turns the internet on. A project
  with no pins gets the installed sources at its first open.
- Must not: create a project from damaged or incomplete input; leave part of a project after a
  failed or stopped import.

### J11 Share the project to Door43

- Actor: facilitator. Activity: Exchange. Serves V3.
- Steps: press Share on the project card · sign in, unless already signed in ("Stay signed in on
  this computer" keeps the token in the computer's password store, ruling H11) · read the author
  notice · choose the owner (your account or an organization) · check the repository name and the
  books · share · see the repository's address.
- End state: the repository exists on Door43 with `main` equal to the project's `main`; the card
  shows that the project is on Door43; the project is unchanged except a checkpoint. Nothing is
  sent to Door43 at start or when the account menu opens. Sign out forgets the token with no
  request.
- Refusals, each with nothing pushed: the name is taken; another computer pushed first; Door43
  cannot be reached; the sign-in failed; the server refused the create.
- Must not: force-push; put the token anywhere but the password store, or in a URL or a log;
  store the password.
- **Differs from tC4:** on a computer with no password store, the sign-in step has no "Stay
  signed in" box and no line about it (#496).

### J12 Upgrade the pinned resources

- Actor: facilitator. Activity: Start. Serves V4, V1.
- Before: J3. Check for updates asks to turn the internet on.
- Steps: open Project Settings · Manage source texts · check for updates · read the offer for each
  set and text · accept.
- End state: the set's pins name the new release; each affected decision is carried over or
  marked invalid and kept; the other set's pins are unchanged; the book text is unchanged. A
  failed or stopped download changes nothing. An original-language upgrade says how many verses
  it touches first, and marks their alignments invalid and keeps them.
- Must not: upgrade silently; move a pin before every resource of the release is installed and
  verified.

### J13 Change the gateway-language resource set

- Actor: facilitator. Activity: Start. Serves V4.
- Before: two gateway-language sets are installed.
- Steps: open Project Settings · choose the other checking language · confirm.
- End state: the primary pins name the new set; the English fallback stays; decisions re-attach
  or are marked invalid and kept. Keep leaves everything as it was.

### J16 Read a passage with helps and write a user comment

- Actor: translator. Activity: Understand. Serves V7.
- Before: J3.
- Steps: open the passage · open a source text · open a translation note · open an Academy
  article · write a user comment.
- End state: one comment bound to the passage; the book text is unchanged. Reading without a
  comment is not this journey.

### J17 Open a shared project on another computer

- Actor: translator. Activity: Exchange. Serves V3.
- Before: a teammate shared the project (J11). This computer does not have it.
- Steps: on Home, "+ Add a project" · "Open a project from Door43" · type `owner/repository` or
  paste its link · "Open project" (it asks to turn the internet on first; Not now sends nothing).
- End state: one new project whose files equal Door43's `main`, with its `origin` set and its
  own author id; its card offers "Sync with Door43"; the first sync needs no other step.
- Must not: add a project after a refusal (a damaged file on Door43, a repository with no
  translationCore project, an unsupported project); leave half a copy after a stop; make a
  second copy of a project this computer already has.
- Differs from tC4: a private repository opens after a sign-in (Q3); the open asks no name, and
  the project keeps its one name (H14); when the project is already here, the app says so and
  offers to open it (H14); the project can also arrive as a file (J27).

### J18 Keep one version of a verse in conflict

- Actor: translator. Activity: Translate. Serves V3, V1.
- Before: two people changed the same verse (or story frame) from the same version, and a sync
  or a file brought both versions together.
- Steps: on Home, click "N conflicts · Review" (or the marker on a verse in Translate) · see
  "Conflict n of m" with each version, newest first, its owner and its date · click one version ·
  "Keep this version" · Previous and Next · "Back to …" at any time.
- End state: the kept text is the verse; the other versions stay in History; Home and Translate
  count only the conflicts still open; the next sync sends the choice to the team.
- Must not: start a sync; write anything when the translator only leaves the review.
- Differs from tC4: only verse and frame text conflict (H5). Check, alignment, comment and
  setting differences never ask; see J28. A passage conflict (verses joined, renumbered or moved
  on one side) is shown too.

### J19 Record findings (Phase 2)

- Actor: consultant. Activity: Check. The end state is defined when Phase 2 starts.

### J20 Create an OBS project

- Actor: facilitator. Activity: Start. Serves V6.
- Steps: choose Open Bible Stories · choose the language, the name and the gateway set.
- End state: a valid OBS Scripture Burrito with fifty stories; each frame holds its picture line
  and an empty paragraph; Home shows the project at 0%.
- Must not: offer a subset of stories; copy source text into a frame; change a picture line.

### J21 Translate a story frame by frame

- Actor: translator. Activity: Translate. Serves V7.
- Before: J20, with the gateway OBS and its pictures installed.
- Steps: open a story · read the gateway frame and its picture · write the frame · write the
  title and the reference line.
- End state: the story file changes only inside the written frame, title or reference line.
- Must not: touch another frame; write two paragraphs into one frame; save a picture line.

### J22 Check an OBS story

- Actor: translator. Activity: Check. Serves V7.
- Before: J21, with the OBS notes and word links installed.
- Steps: open a frame's note or word link · see the source phrase · mark the target words ·
  write a check comment or set a bookmark · preview the drafted stories.
- End state: decisions stored under their story and frame; a later frame edit marks them invalid
  and keeps them; the story file is unchanged.
- Must not: offer an Align tool; write a PDF (that is J23).

### J23 Export an OBS project as Markdown and PDF

- Actor: facilitator. Activity: Deliver. Serves V6.
- End state: outside the project, the stories as Markdown, a dated PDF and a Scripture Burrito.
  The project is unchanged except a checkpoint.

### J24 Share an OBS project to Door43

- Actor: facilitator. Activity: Exchange. Serves V3. The same as J11, with an OBS project.
- Differs from tC4: an OBS project also syncs, like a Bible; its card reads "Sync with Door43"
  (Q2).

### J25 Read a story with helps and write a user comment

- Actor: translator. Activity: Understand. Serves V7.
- Before: J20, with the OBS helps installed.
- Steps: open a story · read the frames with their pictures · read a frame's notes, word links
  and questions · write a user comment.
- End state: one comment per frame commented; the story file is unchanged; Resume returns to the
  same story in Understand.
- Must not: change progress.

### J26 Sync one project with two teammates through Door43

- Actor: translator. Activity: Exchange. Serves V3, V1. Three translators, A, B and C, each on
  their own computer, all with write access.
- Steps:
  1. A drafts and checks, then shares the project (J11).
  2. B and C open it (J17) and work offline. Both change one verse in different ways, and one
     check decision in different ways. Each syncs.
  3. A syncs. The result names B's and C's changes and their Door43 usernames. A reviews the verse
     conflict and keeps one version (J18). The check decision needs no review: the newest wins,
     and the Received sheet says what it replaced (J28). A syncs again.
  4. B and C sync, and get A's choice with no conflict.
- End state: the three projects hold the same project files; each one's text equals Door43's
  `main`; no verse conflict stays open; every change any translator made is on Door43 or in
  History.
- Must not: lose any translator's change; show a check, alignment or setting difference as a
  conflict; leave a verse conflict open after A keeps a version.
- Differs from tC4: saves continue while a sync runs, and a save made during a sync is sent by the
  next one; any default branch works; a dropped connection keeps what was received and says so.

### J27 Exchange a project by file, with no internet

- Actor: translator. Activity: Exchange. Serves V3, V2. Three computers, A, B and C, none online.
- Steps: A saves the project to a file for a teammate · B, who has no copy, receives the file and
  gets a new project · B writes a verse and saves a file back · A receives B's file into the same
  project · C receives B's file as a new project.
- End state: every computer holds every change it received; A's project has B's verse; each file
  is verified when it is made, or deleted. A receive is whole or not at all.
- Must not: lose a change on either side; take in anything the file does not hold; run while a
  sync of the same project runs (the receive waits).

### J28 See what a newer version replaced, and restore the older one

- Actor: translator. Activity: Exchange. Serves V1, V3.
- Before: a sync or a file brought a teammate's newer check decision, alignment, comment or
  setting.
- Steps: read the Received sheet ("Juan's alignment of Titus 1:1 replaced yours. Your version is
  in History.") · open History under the check's marks, the aligner's verse reference or the
  comment · choose "Restore this version".
- End state: the newest version is in place with no question asked; the older one is in History,
  and Restore brings it back and saves it as a new change. A replaced setting names its older
  value, and Project settings can set it again. Two versions of an audio file are both kept, the
  older under its author's name.
- Must not: drop the older version; ask a question for a difference that is not verse or frame
  text (H5).

### J29 Take in a book another program changed

- Actor: translator. Activity: Translate. Serves V1.
- Before: another program changed a book or story file of the project.
- Steps: open the project · answer the one question that lists every changed file · choose Not now
  or Import · later, Import from the review's notices.
- End state: Not now keeps the project's text and keeps each outside file as a copy, listed with
  Import, also after a reload. Import takes the file's text in and reports each book. A refused
  Import says why.
- Must not: overwrite the project's text, or drop the outside file, without the translator's
  choice.

### J30 Read helps from Aquifer

- Actor: translator. Activity: Understand. Serves V4.
- Steps: as J16, with a help set from Aquifer.
- End state: the same helps screens show Aquifer's notes and articles, pinned and offline.
- Must not: need the internet once the set is installed.
- Work: epic #711 (the pilot is #716). No end-to-end test yet; `coverage.json` marks it pending
  on the retired card AQ1, which the coverage check can read offline.
