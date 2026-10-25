# tC4 in Rust: frame and plan

This document frames the rebuild of translationCore 4 (tC4) in Rust. It states what
done means, how big the work is, how much rigor each part gets, and the order of work.
The decision log is `.audit/rebuild.tsv`. The research behind each claim is cited there.

## The goal

Rebuild tC4 in Rust so a translator gets the same outcome as tC4 4.0, plus three things
tC4 4.0 does not do.

tC4 4.0 includes Door43 team sync, "Open a project from Door43" and an Arabic interface
(tC4 decision D102). Team sync in tC4 4.0 is round 1 of its epic #24: one public repository,
the `main` branch only, Bible projects only, one "Sync with Door43" button, a result toast
and a review of verse-text conflicts.

The three things the rebuild adds:

1. Team work goes further. Several team members work on one project offline and exchange
   work later, while others keep working. Nothing anyone wrote is lost. Beyond tC4 4.0,
   the rebuild has:
   - exchange by USB drive or file;
   - saves that continue while a sync runs;
   - private repositories;
   - any default branch, not only `main`;
   - sync of Open Bible Stories (OBS) projects;
   - a review of every conflict kind, not only verse text;
   - carried files, such as audio, that travel with the project.
2. Translation helps can come from Door43 (DCS) or from Aquifer. tC4 plans Aquifer for
   4.1.0 (its epic #711).
3. Every project on disk, and every project that leaves the app, is a valid Scripture
   Burrito.

The guide is `docs/VISION.md` in the tC4 repository. tC4's decisions, its Burrito
specification and its internal structures inform the design but do not bind it. The
user-visible outcome binds it. The tC4 user interface and design system are kept.

### The reference version

tC4 4.0 has no tag yet. Until it has one, the reference is tC4 `main` at commit `d310bf3`
(2026-10-16). When the v4.0.0 tag exists on `release/4.0`, the reference moves to that tag.
`parity/tc4-baseline/README.md` records the commit that its screens show.
`docs/TC4-DELTA-2026-10-16.md` lists every tC4 change from `d494a9d` to `d310bf3`.

### Where the rebuild differs from tC4 4.0 on purpose

The project owner ruled on these points on 2026-10-16 (`docs/TC4-DELTA-2026-10-16.md` §5).

- OBS projects sync like Bible projects. The card reads "Sync with Door43", and the Share
  dialog shows the line about team access. tC4 4.0 syncs Bible projects only (Q2).
- A private repository opens and syncs through sign-in. The app fetches without
  credentials first. When Door43 answers 401 or 404 and nobody is signed in, the app
  shows the sign-in step and tries again with the token. tC4 4.0 opens public repositories
  only (Q3).
- A sync checks write access before it fetches. One API call reads `permissions.push`.
  Without write access the sync refuses, and nothing changes, as in tC4. Other refusals
  after the received work arrived keep that work and say so (Q8).
- The aligner shows the word bank and the words of each card in verse order everywhere:
  from the first view of a verse, after an edit, and after Clear. tC4 at `d310bf3` still
  opens a verse with no saved alignment in word-aligner order (Q15).
- Only verse text and story frame text conflict (owner ruling H5, 2026-10-24). A check,
  alignment, comment, setting, resource pin or book keeps the newest version with no question, as
  in tC4, and the older one stays in History, where *Restore this version* brings it back. tC4
  keeps the older one in its journal where no screen reaches it.
- One damaged incoming file is salvaged. It does not stop the whole sync or open (Q4).
- Every other question in that §5 follows its recommendation.

Under AGENTS.md "Matching tC4" (2026-10-19):

- Words inside nested alignments show in Translate, Check, Align and print, such as Genesis
  1:1 "the heavens" and a Psalm's "Selah" aligned inside `\qs` (tC4 #782, on tC4 main at
  9ad4d5c). The reference d310bf3 drops them (Q32 of `docs/TC4-DELTA-2026-10-19.md`).
- A footnote, cross reference, figure or any other marker the shown text leaves out (`\va`,
  `\vp`, `\ca`, `\rq`, `\cat`, `\ts\*` …) between two words keeps the space after it, so the
  words never join and can be checked, aligned and exported. tC4's open PR #800 does this for
  notes, cross references and figures only (C46). Where the marker is followed by a letter
  with no space (`siervo\f + \ft o esclavo\f*de`), the shown text puts a space between the
  two words, as the export splits them; tC4 shows "siervode". So it does where a letter
  follows the start or end of a character span or a paragraph marker directly:
  `the \add Lord\add*s house` shows "the Lord s house", `Yahweh\q2 que` "Yahweh que" and
  adjacent ruby `\rb 神|かみ\rb*\rb 愛|あい\rb*` "神 愛", where tC4 joins "Lords",
  "Yahwehque" and "神愛", which the export cannot weave.
- The words of a character span after a span nested in it show in Translate, Check, Align and
  print: `\wj Worship the \+nd Lord\+nd* your God\wj*` shows "Worship the Lord your God",
  where tC4 shows "Worship the " and hides from Check and the Align bank words the export
  counts (#796).
- The words after a character span that holds a footnote, cross reference or figure show:
  `\wj Blessed are the poor\f + \ft Or humble\f* in spirit\wj* and he went on` shows "… in
  spirit and he went on", where tC4 hides "and he went on" (usfm-js keeps it as the `content`
  of the span's end marker), so Check and the Align bank miss it, a woven alignment for the
  verse cannot be made, and the store's verse save drops the links to it. So does the text
  after a stray end marker (`a\nd* b`). The note's own words stay hidden.
- Align, across a verse edit, moves each link with its own word, by position: deleting or
  adding an earlier copy of a repeated word leaves the link on the word it was on. A link
  whose word cannot be followed for sure goes back to the word bank and is listed, and the
  verse is not marked for review (#789, Q30 (a)). tC4 4.0 moves the link to another copy of
  the word, unannounced (#789 is open in tC4).
- Align opens a verse with a card for every word of the pinned original-language verse,
  comparing words in NFC; a link to a word the pin lacks goes back to the word bank and is
  listed, and the verse needs review on the rail and when it opens (#790). tC4 shows the stored cards as they are, so the sample's TIT 1:1 offers 11
  of UGNT's 17 Greek words. Suggestions compare original-language words in NFC (#793), so
  Hebrew alignments imported from tC4 4.0, which stores them in NFC, train suggestions for
  the pinned UHB.
- Align's "The draft changed" notice and the rail's Invalid follow one rule: the text, the
  words, the span or the original-language text changed, or the alignment is marked for
  re-review (#787, #796). tC4 shows the notice only for a changed text or changed words, and
  its rail calls an alignment valid when only its words are wrong.
- A carry-over that loses no link is saved when the translator leaves the verse
  (APP-DESIGN AD19); tC4 writes nothing at open (#787).
- Align's cards are a Tab stop while words are chosen, and Enter or Space places them, so a
  person who cannot drag or point can align (#568); tC4 4.0 has no key for it. Every card also
  shows the pinned verse's words (#790, above).
- A verse that still holds the alignment markup of the file it was added from does not
  open in Align, and no verse save writes an alignment over it (#794). This is tC4's open
  PR #799.
- The items of a `\li1` list, a `\qm1` line, a table cell and the text of a marker usfm-js
  does not know (a typo such as `\pp`, a custom `\zx`) show, where tC4 hides them (usfm-js
  keeps their text as `content`) while the export counts their words: UST EXO 15:21
  keeps the 28 words the import placed at open and at the next verse save, where tC4's text
  drops 11.

Under the same rule, in the conflict review (B11 and C7a, 2026-10-23):

- The Text tab's conflict says when one set of changes made other conflicts ("Changed together
  with 2 other conflicts"), when some earlier changes of a teammate have not arrived, and when
  a version cannot be read on this computer yet. tC4's review has none of these lines; each
  names a reason a choice should wait.
- A passage conflict (one person joined, renumbered or moved verses while another changed one
  of them) is one conflict over every verse of the passage, with *Keep <person>'s passage* for
  each side: every verse of the passage as that person left it, written in one batch over one
  token per verse, so no verse of the passage is written over a version the screen did not
  show. A conflict of verses joined or split differently offers *Keep the joined verses* and
  *Keep the verses separate*, with the version clicked as the pick where a verse holds several.
  tC4 reviews verse text only and keeps the newest structure unasked.
- History has *Restore this version* on every version but the one shown: the old version is
  kept over every version of the record, as a choice on the conflict, so the conflict is
  settled on it and every other version stays in History. tC4 has no History (#78).
- A version of a passage conflict names its verse when it is not the conflict's ("Titus 1:5"
  on the card). *Keep this version* is also offered on a book header, a chapter opening and
  a story's text, frame and reference line, which tC4 does not review; the line over the
  versions names the kind ("Passage · 3 versions") where tC4 always counts "versions of this
  verse"; and a passage's version cards cannot be clicked, since a passage is kept by side.
- *Keep all of <person>'s* keeps one person's version over every conflict of the tab where
  that person's computers hold exactly one of its versions; a conflict where they hold two, or
  none, is left for a choice of its own, and the result says how many. There is no "keep the
  newest" bulk choice: it would hand the decision to the clock.
- A refused choice says why in words ("Another version of this verse is kept.", the store's
  reason), never as an error code.

Under the same rule, tC4 4.0 bugs the rebuild fixes on purpose (FIX-R, 2026-10-23):

- A project's text direction has one source, the metadata's `scriptDirection`, else the
  settings file's `textDirection` (tC4's own `projectPresentation` rule). Project settings
  writes both, so a direction changed there shows on Home, in the open project and in Align,
  also after a restart. tC4 writes only the settings file, so the change shows nowhere
  (#832.1). As in tC4, a metadata value other than `ltr` or `rtl` does not count. Settings names the language from the metadata when the settings file names none,
  where tC4 shows an empty field for a tC4 project (#832.2).
- The words placed on an aligner card read in the translation's direction, even on an Old
  Testament verse; tC4 lays them out in the original's direction, so a Hebrew verse's
  placed words read backwards (#830).
- Each note, word and question text reads in its own direction under its pane's layout, so
  an English question in an Arabic set is not laid out right to left (#836).
- A save refused for a reason no retry in this open can fix (a bad token, a newer change)
  keeps its text for the next open's offer and does not block going Home; tC4 keeps Home
  blocked (#801). A text refused for its content still shows why beside the field.
- A failed check for updates says one plain sentence by cause; tC4 shows the browser's
  "Failed to fetch" (#833.1).
- Dropbox's own settings folder (`~/.dropbox`) never marks a project as synced, so having
  Dropbox installed on Linux or macOS no longer makes every project read-only.
- Align lists the links its open sent back to the word bank (owner answer Q30 (a)); the list
  after a verse save in Translate waits for the storage unit S2.

These differences follow the owner's rule in AGENTS.md ("Matching tC4", 2026-10-17), and the
owner is told of each:

- Urdu in Devanagari script (`ur-deva`) reads left to right and is named उर्दू (#701). tC4 4.0's
  registry says right to left and names it اردو in Arabic script, so its helps would read
  backwards. The Source texts screen draws that row in Devanagari; B9a explains it with a residual
  of `source-texts` that cites `#701` (APP-DESIGN §7.5): the row lies below the dialog's visible
  part, where no mask can cover it.
- A story project's own pictures are found by their path below `ingredients/`, outside
  `checking/`, `tc/` and `content/` (#549). tC4 4.0 takes only an ingredient with the role
  `x-obsimages`, which the rebuild's `metadata.json` does not write for a carried file and
  Pankosmia's rescan drops, so a project's own pictures would never show.
- A Select that turns disabled while its list is open closes the list, and a choice made then
  changes nothing (#552). tC4 4.0 keeps the list open, and a row click still changes the value.
  The gallery's three `Select/disabled-while-open` cases differ from tC4 on purpose
  (`parity/ds-gallery/residuals.json`); the `-script` one chooses the row in the same task as the
  switch-off, before the list can close.

The owner's answers of 2026-10-19 (`docs/TC4-DELTA-2026-10-19.md` §5) and the rebuild's own
rule that no one's work disappears without a word add these (unit B2):

- Home lists every folder that holds a project's files but cannot be opened, with its name
  and the reason, and offers nothing to press. tC4 4.0 leaves such a folder off Home.
- Choosing a language whose Door43 code a Scripture Burrito cannot hold (such as
  `acm-x-lebarabic`) fills the Code box with a code it can hold that keeps every letter
  (`acm-x-lebarabi-c`), and one sentence says so. tC4 4.0 stores "Undetermined" (#582, Q35).
- A book tile counts a verse as drafted when it has words, whatever line they are on. tC4 4.0
  reads a verse only up to its first paragraph marker, so a poem verse whose words are on
  `\q` lines counts as not drafted there (#572).

And these (unit B10):

- After a sync, the result names the verses that teammates aligned differently ("1 verse has
  two alignments to review."), and Review opens them. tC4 4.0 keeps the newest alignment and
  counts it among the conflicts that kept the newest version, so a teammate's alignment can
  disappear unannounced (#791).
- A sync that stops for a reason no sentence names says so in one fixed sentence. Door43's
  words and the reason, which can name a file on the translator's computer, stay in the
  problem report. tC4 4.0 shows "The sync stopped: {reason}" (#724, Q36).

And these (unit C6):

- A book is added from a USFM file in one batch, whole or not at all, and the alignments of an
  aligned file are kept (#767, #553). tC4 4.0 writes the stub book first and the verses after, so a
  stop between leaves a stub book that every retry refuses, and the app never refuses a book for
  its size.
- Add a book does not warn that creating a book the project holds again "will replace the
  existing book and its drafts". Adding never replaces a book, in tC4 as here.
- A new project's language is named by what the person typed, else by the shipped Door43 list,
  else by its code. tC4 4.0 took the name from the platform's own table. The Door43 code a
  person was offered another for is kept in the project's settings (`door43Language`), since a
  Scripture Burrito has no field for it (Q35).
- A project whose projects folder cannot keep a copy safe is copied to this computer's local
  projects folder, and Home lists it there (Q-30). tC4 has no such copy.

And these (unit B4b):

- While a section card holds text it has not saved, the save indicator reads "Unsaved
  changes". tC4 4.0's indicator follows only its verse saves and reads "Saved" then, even
  when every word of the section has just been deleted and nothing is stored. The M18 case
  `section-empty-confirm` shows it, with a residual.
- A paragraph or poetry line before a verse with no words stays before that verse when the
  section is saved: the format typed on that verse's line, or the line it had when the text
  gives it none. tC4 4.0 gives that line to the next verse with words, so a section save that
  left a seeded book's verses undrafted moved or removed the lines the book took from its
  source. An explicitly typed number-only line keeps its requested format through Place
  mode and pin moves too; omitting that line keeps the stored format, while an explicitly
  unindented line removes it. tC4's serializer omits number-only lines.
- When a section's first verse has no words, the text a Place session writes keeps the format
  of its first line, so a save from Place keeps it. tC4 4.0's `serializeDraft` drops it (its
  own Save reads the session, not the text).
- A typed span followed by an empty pin inside it keeps every other pin through Place and
  back to Type. For `1-4 0` followed by bare `2`, pins 1, 3 and 4 stay on the word and pin 2
  stays in the bank. tC4's serializer drops pin 1. The rebuild writes the covering span and
  then its empty interior override, preserving verse boundaries as well as words and formats.

And this (unit C2):

- Section Cancel waits until the server durably removes that card's current draft. A failed
  Cancel keeps the card and text and offers Retry, so a cancelled draft cannot return as a
  recovery offer after a crash. tC4 4.0 closes the local card immediately and has no equivalent
  durable draft removal.

And this (unit C3):

- A story text the story file cannot hold, once tC4's own clean-up has run (blank frame lines,
  carriage returns and a title's or reference's outer spaces go, as in tC4), is not saved, and
  the save indicator says why beside Retry while the card keeps the text: an emptied reference
  line ("The reference needs text."), or a frame line that would read back as the picture,
  title or reference line. tC4 4.0 puts the saved reference back into an emptied box without
  a word, and shows only "Save failed — retry" for the others, whose retry can never succeed.

And this (unit B11):

- The conflict review lists, besides tC4's verses whose text two people changed, the records
  that lost their place and the notices, each in a tab of its own; under H5 the Alignments,
  Checks and Settings tabs hold only an edit made outside the app with no known base. The tabs
  show only while one of them holds something, so a project with verse conflicts alone shows
  tC4's review.
- The Received sheet says what a newer version replaced: "Juan's alignment of Titus 1:1 replaced
  yours. Your version is in History." tC4's sync result only counts "{n} conflicts kept the
  newest version" (H5).
- Two versions of an audio or other carried file are both kept. The newest keeps the name, the
  other is saved beside it under its author's name, `jon_01-Juan.wav` (`-2`, `-3` while the name
  is taken). tC4 does not journal `audio/` and keeps one copy (H5 rule 3).
- A check that moved (a carry-over) keeps the move at its old place even when a teammate's
  decision there is newer; that decision is listed in the History of the check it moved to.
  Newest-wins there would show the check twice. tC4 keys decisions without moves.
- A file whose name differs only in letter case from another keeps one name by byte order, never
  by which is newer; the other is placed beside it. A newer recording that took the name would,
  on Windows and macOS, read the other file's bytes. tC4 leaves this to git.
- An edit made to a project file outside the app with no known base never wins by date alone: it
  stays a question in the outside edit's review, so nothing the person wrote is dropped unasked.
- A newer removal of a book removes it, and its edits stay in History; a newer edit keeps it; the
  last book of a project is never removed by a sync (H5 rules 4 and 5).
- A version dated more than a day after this computer's clock says the clock it was made on
  may be wrong, and the review has a History of the verse (#78). tC4 4.0 has neither.

The rebuild does not port tC4's sync mechanism: the scratch copy, the apply marker, the
ops log, `recoverySettled`, `unreadableSettings` and the sync generation. STORAGE §6 and §9
give the same outcomes, and STORAGE §9.9 keeps saves flowing while a sync runs. tC4 can
leave a torn `metadata.json` (tC4 RISKS #13). The rebuild cannot, because it writes every
file atomically (STORAGE L18).

## Definition of done

The rebuild is done when every statement below is true and each one is shown by a
check in CI that can fail.

- **P1. Journeys.** Every live tC4 journey has an automated end-to-end test that drives
  the Rust app through its UI in a browser engine and asserts the journey's
  user-visible end state and its on-disk end state. The live journeys are J1 to J8,
  J9a, J9c, J9d, J11 to J13, J16, J17, J18 and J20 to J26 (tC4 `docs/JOURNEYS.md`).
  Where tC4 asserts an internal file shape, the Rust test asserts the equivalent fact in
  the new format.
- **P2. Team work.** These journeys pass end to end:
  - J17: open a shared project on another computer, then continue offline. The project
    comes from Door43, through a Door43-compatible git server, or from an exchange file.
  - J18: keep one version of a conflicted verse. As in tC4, step 3 of J26 can prove it.
  - J26: sync one project with two teammates through Door43, against a Door43-compatible
    git server. Three users edit offline at the same time. The steps are tC4's. The
    rebuild adds one: A also resolves the conflict between two check decisions.
  - The rebuild's own journey: exchange by USB file in both directions, while both sides
    keep editing. It needs no server.
- **P3. No silent loss.** A property test generates random concurrent histories for
  three or more devices and merges them in random orders and groupings. It shows that
  every merge order gives the same state and that no operation disappears without
  being shown. Every concurrent change to the same verse, alignment or check decision
  either merges without loss or appears as a conflict the user can see. Each silent-loss
  case found in tC4's design is a named regression test.
- **P4. Crash safety.** A test kills the writer at every write point of save, sync,
  import and checkpoint. After each kill, reopening the project gives the state before
  the action, the state after it, or, for a sync or an import, a valid state between the
  two in which each received batch is whole or absent (STORAGE §6.4, rows Y2 and I2).
  Nothing is lost and nothing is corrupt. For a sync this is also tC4's G8: after a kill
  or a power loss at any point, the project opens, no own change is lost, no teammate's
  change is reverted or given to another person, and the next sync converges with
  Door43.
- **P5. Valid burritos.** Every project state written by any test is validated against
  the Scripture Burrito 1.0 schema plus the checks the schema misses: ingredient md5
  and size, path rules, and scope coverage. This includes every commit, export and USB
  file. There are zero failures.
- **P6. Helps sources.** The same helps screens work with DCS resources and with
  Aquifer resources, pinned and offline, using recorded real data as fixtures.
- **P7. Same look.** The key screens are screenshotted in the Rust app and compared with
  the same screens in tC4, with the interface in English (`en`) and in Arabic (`ar`). The
  tC4 screens are in `parity/tc4-baseline`, captured at the reference version. A human
  reviews the differences, and the differences are listed.
- **P8. Scripts.** Arabic and Hebrew text renders right-to-left and edits correctly in
  the browser engine used by tests. The app's own interface is right-to-left in Arabic,
  as in tC4 4.0, not only the scripture text. Graphite rendering is checked by a probe that
  also measures vertical glyph extent. That probe runs in the Electronite shell on the
  release pipeline for Windows and macOS. Electronite's Linux build also runs in this
  container under Xvfb (G0, 2026-10-17), so the probe can run here too.
- **P9. Carry-over.** A tC4 4.0 project, a tC3 project, raw USFM and a Scripture Burrito
  each import into a valid project with their text unchanged byte for byte. The tC4 4.0
  cases include:
  - a project that has synced with a team: its name events, a local history that differs
    from Door43 `main`, and the folders a sync leaves behind, which the import skips;
  - a project whose sync apply stopped part way: the import refuses it and says to open
    it in tC4 once, so tC4 can finish its sync (Q23);
  - a torn `metadata.json`;
  - the stamps that the platform writes into `metadata.json`.

- **P10. Old projects keep working.** Every format version, once frozen, leaves a set of
  sample projects written by that version, which nobody may change afterwards. CI opens every
  one with the current build and shows that every verse, alignment, check decision, comment,
  conflict and history entry is still there and unchanged, that the project still validates,
  and that it can still be edited, saved and exchanged. CI also has the last frozen version
  open projects the current build wrote: it edits them, opens them read-only with a clear
  message, or refuses a newer major before writing, and never calls them damaged. A test that
  pins the format's bytes fails when the format changes without a new version and new sample
  projects (owner request, 2026-10-18; STORAGE §12, storage unit V).

## Scope

tC4 is about 43,000 lines of product code, made up of these parts:

- 8,000 lines of views.
- 4,400 lines of design system.
- 7,000 lines of application state.
- 18,500 lines of data layer.
- 5,100 lines of journal reference code.

It also has 43,000 lines of unit tests and 14,000 lines of end-to-end tests. The Rust
rebuild is expected to be 50,000 to 70,000 lines across about 15 crates. That number is
an estimate, not a measurement.

These constraints come from this build environment:

- The network policy blocks `git.door43.org`, `api.aquifer.bible` and `api.github.com`.
  Door43 behavior is therefore tested against a local git server and a fake Gitea API
  built from the Gitea swagger file and tC4's recorded fixtures. Live Door43 calls are
  open until a run on a machine with access.
- Electronite's Linux build runs here under Xvfb, as root with `--no-sandbox` (G0,
  2026-10-17, which saw Awami Nastaliq joined by Graphite). The Graphite proof on Windows
  and macOS is open until it runs on the release pipeline.
- The bundled English suite lives in `bundle/en/`. The OBS image pack is too large for git,
  so the release pipeline downloads it at its pinned commit and checks it against
  `bundle/en/fetched.lock`. The installer carries it, so nobody who installs the app
  downloads it. Read `bundle/en/README.md`.

## Rigor

Rigor is high wherever a mistake loses a translator's work or locks in a format:

- the journal and the merge;
- storage and crash recovery;
- sync and USB exchange;
- import;
- the Burrito writer.

These parts get property tests, crash-injection tests and an adversarial review by
several models before they merge. The UI port gets end-to-end journeys and screenshot
comparison. Resource parsers get tests against real fixtures from tC4 and Aquifer.

## Architecture

- **Core.** Rust library crates own all data: USFM, versification, the Burrito,
  the journal and fold, the project store, resources, checking, alignment, OBS,
  import, export and sync. No other process writes the project.
- **Server.** A Rust HTTP server on `127.0.0.1` exposes the core to the UI, guarded by a
  per-launch token. It serves the UI files.
- **UI.** Written in Rust and compiled to WebAssembly. It reuses tC4's CSS, design
  tokens, fonts and translation catalogs, so screens look the same. API types are
  shared with the server in one crate, so the two cannot drift.
- **Shell.** In production the window is Electronite, because it is the only shell
  shown to shape Graphite fonts. In development and tests any Chromium-based browser
  works.

Details and the reasons for each choice are in `docs/ARCHITECTURE.md`.
