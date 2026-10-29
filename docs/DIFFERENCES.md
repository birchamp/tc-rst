# Differences from tC4 4.0

**Answers:** where the app differs from tC4 4.0 on purpose, and why. **Wins over:** nothing;
rulings and `docs/VISION.md` decide, and this file records the result. **Does not cover:** new
features, which tC4 never had (`docs/JOURNEYS.md`).

tC4 4.0 is history (ruling `docs/rulings/2026-10-29-tc4-is-history.md`). The journeys J1 to J26
and the screens came from it, and for 4.1.0 the app's screens were compared with tC4's at the
reference version (`docs/rulings/2026-10-27-p7-review.md`). That comparison is retired. Each entry
below is a place where the app chose otherwise than tC4, so a person who knows tC4 can find what
changed and why.

When you change a screen or a behavior that came from tC4, add an entry here in the same pull
request. End it with the issue or the owner's answer in parentheses, for example `(#701)` or
`(Q3)`.

## The reference version

The owner chose tC4 4.0 rc.2 (H7, 2026-10-24): the reference is tC4 `release/4.0` at commit
`d25b7871` (2026-10-22, version 4.0.0-rc.2), the tag `v4.0.0-rc.2` (`a5ce0307`) plus the fixes
picked onto `release/4.0` after it, among them the MIT licence (#838). When the v4.0.0 tag
exists on `release/4.0`, the reference moves to that tag. `parity/tc4-baseline/README.md`
records the commit that its screens show. `docs/TC4-DELTA-2026-10-16.md` lists every tC4 change
from `d494a9d` to `d310bf3`, and `docs/TC4-DELTA-2026-10-23.md` (SWEEP) the changes after it.

## Where the rebuild differs from tC4 4.0 on purpose

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
- Only verse text and story frame text conflict (owner ruling H5, 2026-10-24). A check,
  alignment, comment, setting, resource pin or book keeps the newest version with no question, as
  in tC4, and the older one stays in History, where *Restore this version* brings it back. tC4
  keeps the older one in its journal where no screen reaches it.
- One damaged incoming file is salvaged. It does not stop the whole sync or open (Q4).
- Every other question in that §5 follows its recommendation.

Under AGENTS.md "Matching tC4" (2026-10-19):

- The help pages name this app's projects folder, `~/translationCore4/projects`, and its
  download page, https://translationcore.com/tc4, where tC4's pages name
  `~/pankosmia/tc4-projects` and tC4's GitHub releases (H16 parts 2 and 3). The vendored pages
  stay tc-website's bytes; `tc4 serve` makes the change as it serves them (tc-server
  `HELP_OVERLAY`, REF1).

- A footnote, cross reference, figure or any other marker the shown text leaves out (`\va`,
  `\vp`, `\ca`, `\rq`, `\cat`, `\ts\*` …) between two words keeps the space after it, so the
  words never join and can be checked, aligned and exported. tC4 PR #800 (merged, in rc.2) does
  this for notes, cross references and figures only (C46). Where the marker is followed by a letter
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
  verse is not marked for review (#789, Q30 (a)); so when one copy of a repeated linked word
  is replaced, both links go back (#820). The reference d310bf3 moves the link to another copy
  of the word, unannounced. Since #815 (D106, in rc.2) tC4 also follows a link by position, but
  where an edit before a linked repeated word is not clear it marks the verse for review
  instead of listing the link (#789, #820).
- Align opens a verse with a card for every word of the pinned original-language verse,
  comparing words in NFC; a link to a word the pin lacks goes back to the word bank and is
  listed, and the verse needs review on the rail and when it opens (#790). The reference
  d310bf3 shows the stored cards as they are, so the sample's TIT 1:1 offers 11 of UGNT's 17
  Greek words. Since #816 (D107, in rc.2) tC4 also repairs the cards from the pinned verse; it
  also marks for review a done alignment that only gains empty cards, which the rebuild does not. Suggestions compare original-language words in NFC (#793), so
  Hebrew alignments imported from tC4 4.0, which stores them in NFC, train suggestions for
  the pinned UHB.
- Align's "The draft changed" notice and the rail's Invalid follow one rule: the text, the
  words, the span or the original-language text changed, or the alignment is marked for
  re-review (#787, #796). The alignment files' `invalid` follows the same rule, so a teammate
  on tC4 reads such an alignment as needing re-review too. The reference d310bf3 shows the
  notice only for a changed text or changed words, its rail calls an alignment valid when only
  its words are wrong, and its files write `invalid: false` there. Since #814 (D105, in rc.2)
  tC4 uses one rule too. Only Mark valid clears "needs review", in tC4 and, by the owner's
  ruling of 2026-10-23 (Q37), in the rebuild (FIX-R).
- A note's quote is marked in the original-language text as well as in the gateway text: in
  Draft's Greek or Hebrew tab and in the original row of the check card, at the note's
  occurrence; a quote that does not match in full marks nothing. tC4 4.0 marks only the
  gateway text (#840; owner ruling Q39, 2026-10-23). Understand shows no original-language
  text, in tC4 or the rebuild, so it has nothing to mark.
- A carry-over that loses no link is saved when the translator leaves the verse
  (APP-DESIGN AD19); tC4 writes nothing at open (#787).
- Align's cards are a Tab stop while words are chosen, and Enter or Space places them, so a
  person who cannot drag or point can align (#568); tC4 4.0 has no key for it. Every card also
  shows the pinned verse's words (#790, above).
- A verse that still holds the alignment markup of the file it was added from does not
  open in Align, and no verse save writes an alignment over it (#794). This is tC4 PR #799
  (merged, in rc.2); the rebuild still shows its own words for it until REF1 takes tC4's.
- A verse whose text starts on the line after its verse number opens in Align and can be
  aligned. Since #800 (in rc.2) tC4 refuses it with "tC4 cannot save an alignment for this
  verse" and asks the translator to move the text onto the verse number's line.
- The items of a `\li1` list, a `\qm1` line, a table cell and the text of a marker usfm-js
  does not know (a typo such as `\pp`, a custom `\zx`) show, where tC4 hides them (usfm-js
  keeps their text as `content`) while the export counts their words: UST EXO 15:21
  keeps the 28 words the import placed at open and at the next verse save, where tC4's text
  drops 11.

Under the same rule, for checkpoints and files changed outside the app (D1, owner ruling OE 3
of 2026-10-21, `docs/STORAGE-DELTA-2026-10-21.md` §2.11, §2.17):

- A book file another program changed is kept and asked about, at open, when the watcher finds
  it and when the window gets focus: "This book has been changed externally. Would you like to
  import it into this project?" with *Import* and *Not now*. tC4 takes an out-of-band USFM edit
  in on its own at the next open (`reconcileDivergence`), has no watcher or focus check, and
  refuses its checkpoint while the project is open ("A file changed outside the app…").
- A story file another program changed is asked about the same way. tC4 refuses to open the
  project (`open.story-divergence`); a story must not lock the project.
- *Not now* writes the file again from the project and keeps the outside file as a copy in
  `.tc/external`, listed in the conflict review's notices with *Import* and *Dismiss* until one
  is chosen; the toolbar's conflicts button counts these copies too, where tC4's counts verse
  conflicts only. No *Undo* follows an import (delta Q7).
- A mode switch checkpoints at most once every two minutes after one that committed; leaving
  the project always checks. tC4 checkpoints at every mode switch (Q-5's fallback, A-11, until
  P reports R9).
- The checkpoint commit message, visible in Door43's history, ends with the books:
  "Checkpoint, leaving Translate: TIT (tC)", where tC4's reads "…: TIT text (tC4)".
- Work found only in a book or story file the app wrote (a save missing from the project's
  records) is asked about once: "Some recent work was missing from this project's records.
  Bring it back?" with *Bring it back* and *Not now*; after *Not now* the review lists it with
  *Bring it back* only, never *Dismiss*. tC4 takes it in on its own at the next open
  (`reconcileDivergence`) (H22 part 1).

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
  keeps its text, or the aligner's links, for the next open's offer and does not block going
  Home; tC4 keeps Home blocked (#801, #807). A text refused for its content still shows why
  beside the field, and a full disk still holds the person where they are.
- Home with an aligner click the server has not taken waits up to 3 s while the click is on its
  way or sent again, and leaves once it saves; a click refused for good, or still unsaved then,
  asks Stay or Leave without it. tC4 neither waits nor asks (owner ruling ALIGN-LEAVE,
  2026-10-26).
- A failed check for updates says one plain sentence by cause; tC4 shows the browser's
  "Failed to fetch" (#833.1).
- Get source texts names under each package the notes, words and academy releases installed
  on this computer (for the project's package, the installed release it pins), and tC4's
  registry release for one not installed; tC4 always names its
  registry's, so English said v89 while v91 was installed and pinned (#833.5).
- Dropbox's own settings folder (`~/.dropbox`) never marks a project as synced, so having
  Dropbox installed on Linux or macOS no longer makes every project read-only.
- Align lists the links its open sent back to the word bank (owner answer Q30 (a)), and
  Translate lists the links a verse or section save sent back, with the verse, until the person
  dismisses them (LINK-DROP, tC4 #825 and #820); tC4 changes the alignment after a save and says
  nothing. The Translate list lives in the page: a crash between a save's write and its reply
  loses the notice, while the old link stays in History.

Owner rulings in the issues, made before 4.1.0:

- On a computer with no keychain that keeps a sign-in, the sign-in step has no "Stay signed in
  on this computer" box and no line in its place, and the Share dialog has no line about it
  either (#496, owner ruling 2026-09-30). tC4 shows "Stay signed in is not available on this
  computer, so your sign-in lasts for this app session." (`signIn.notKept`), which no screen of
  the rebuild shows. The key stays in the vendored tC4 catalogs, which are tC4's bytes.

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
- A verse counts as drafted when it has words, whatever line they are on, on Home's book
  tiles, in Check and in Align alike, so a Psalm verse written on `\q` lines can be checked
  and aligned and its decision is saved (owner ruling H12). tC4 4.0 reads a verse only up to
  its first paragraph marker, so such a verse counts as not drafted there: Check shows it with
  no words to tap and Align gives it no text (#572).

And these (unit B10):

- After a sync, the result names the verses that teammates aligned differently ("1 verse has
  two alignments to review."), and Review opens them. The reference d310bf3 keeps the newest
  alignment and counts it among the conflicts that kept the newest version, so a teammate's
  alignment can disappear unannounced (#791). Since #806 (D108, in rc.2) tC4 names each such
  verse in its own row ("{book} {ref}: {name}'s alignment replaced yours."), but still keeps
  only the newest alignment; the rebuild keeps both. Owner ruling H5 (2026-10-24) changes this:
  the newest alignment wins, the older stays in History, and the Received sheet says what was
  replaced (unit C7b); until C7b lands the toast still counts such a verse.
- A section draft not yet saved survives a reload, and its card opens again with the typed
  text when Translate shows its chapter, ready for Save or Cancel. tC4 4.0 keeps the draft only
  in the open card, so a reload or crash loses it (FIX-HIDE2).
- When Door43 no longer has a project's branch (its owner deleted it), the next sync puts this
  computer's copy back and says so: "The project was missing from Door43, so tC4 put your copy
  back. Ask your teammates to sync." tC4 4.0 pushes the branch again and says nothing (owner
  ruling of 2026-10-24; evidence/2026-10-21/gap-tc4-straggler-sync-safety.md state 3b).
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
  yours. Your version is in History.", one line per record a newer version replaced, of every
  kind. tC4's sync result counts "{n} conflicts kept the newest version" and lists only the
  replaced alignments of the person's own (#791) (H5).
- History with *Restore this version* sits under a check's marks, under the aligner's verse
  reference and under a verse or frame comment (HIST-1). A project setting (the text font or
  direction) or a resource pin has no History list: its Received row names the older value
  ("Juan's text font replaced yours (was Noto Sans). Project settings can set it back."), and Project
  settings sets it again, because those dialogs already offer the older value as a choice
  (orchestrator, 2026-10-25). The Home card's "Last sync: n versions replaced" shows the last
  sync's rows again after the toast and after a reload, until the app quits. tC4 shows them
  only in the toast.
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

And these (unit D2):

- Aligned USFM weaves a verse's alignment only when the verse needs no review and its words
  match. Any other verse keeps its stored text and markup, and the export names it (at most
  20, then the total). This is tC4's rule from e2bfd8c3 (#796), which v4.0.0-rc.2 also has
  through #814 (7baa1257: `cc.exportSkipped` and the `export-skipped` callout). Both are newer
  than the reference d310bf3, which leaves such an alignment out of the file without a word.
- An aligned word inside a character span such as `\add` keeps the span in aligned USFM
  (#420). tC4 4.0 moves the word out of the span, so the file loses its style; its v4.0.0
  notes list this as a known limit (tC4 D89 point 11).
- Plain USFM is the aligned book with its alignment markup taken out. tC4 exports the stored
  book file byte for byte. The two differ only for a verse imported with inline `\zaln`
  (#794) whose markup strips, which plain USFM writes without that markup. A verse whose
  markup will not strip (an unclosed `\zaln-s`, a `\w` with another attribute) is written as
  stored, as tC4 writes it, and named under the toast.
- The PDF's print page is titled with the export's file name without `.pdf`, because the
  shell names the saved PDF from the title (`tc4Shell.printPdf`). tC4's print document is
  titled with the book's or the project's name. The printed heading is the same.
- An export of a book or the stories with verses in conflict first asks, under the menu,
  whether to review them, with Review and Export anyway (STORAGE §8.5). tC4 exports the
  version shown without a word, so the person cannot know the file holds only one of two
  versions. The question is inline, never modal.
- A verse whose cards stand on a source word the pinned verse lacks keeps its stored text,
  and the export names it. tC4 #816 (D107, newer than d310bf3) fits the cards to the pin and
  weaves the rest, so a link leaves the file without a word to the person; the rebuild weaves
  none of that verse until it is aligned again
  (`the_export_weaves_no_link_to_a_source_word_the_pin_lacks`).

The rebuild does not port tC4's sync mechanism: the scratch copy, the apply marker, the
ops log, `recoverySettled`, `unreadableSettings` and the sync generation. STORAGE §6 and §9
give the same outcomes, and STORAGE §9.9 keeps saves flowing while a sync runs. tC4 can
leave a torn `metadata.json` (tC4 RISKS #13). The rebuild cannot, because it writes every
file atomically (STORAGE L18).
