# Plan for design unit B4b: the section editor

Planned 2026-10-19 on branch `lane-b4b`, cut from `claude/focused-ride-5uzj7p` at `3889aad`.

The row (docs/APP-DESIGN.md §10, phase 1): `model::SectionBuffer` (port of `sectionDraft.js`);
the section card with Type and Place, pins and the "To place" bank; Save only, Cancel, the #600
question (M18); section drafts; the editing lock for sections. Tests: `sectionDraftExits` port;
section buffer proptest and its tc-app differential test; `section_editor_saves_only_on_save`;
Cancel leaves no buffer entry and no draft; the #600 question. Done when:
`en-translate-section-editor`; J2's section steps on scripted saves. Issues named B4b in
TC4-ISSUES: #600 (emptied section asks, then writes stubs), #598 (pin: click, then click), #604
(the idle Place hint), #575 (a joined span has no blank line), #614 (VerseMarker's label from the
catalog), #786 (a no-change section save on Psalm 3 and GEN 3:14-17 writes nothing).

R6 (a `Draft` of a section carries every displayed token) is provided by S1, so the envelope is
not built. The `Draft` itself is sent by C2's `editor/draft`, as the verse card's is.

What exists and is reused: A4b's save machine already has `FieldKind::Section` with
`SaveRule::OnSaveOnly` and a draft on blur and 2 s; `Saves::discard` is its Cancel; the
`Editing::Section` lock; B4a's grid, "Draft section" pill (off while a card is open),
`editing_card`, the `test/saveVerse` writer pattern; ds `VerseMarker`, `Switcher`, `Modal`; the
catalog keys `draft.*`; tc-store's `SaveSection` (STORAGE §5.2) with its coherence check and the
reflow in the same batch; the gallery case `section-empty-confirm` (M18).

## Design decisions

- **The field's text is the Type-mode text, always.** Place mode edits a parse of that text; a
  drop writes the text again (`serialize`). So the browser buffer, the shell's mirror and the
  server draft hold the whole card in one string, a crash in Place mode keeps the placements, and
  the save machine needs no new kind of value. Switching Place to Type normalizes the text as
  tC4 does (`serializeDraft`).
- **`tc_api::model::section::SectionBuffer`** is the parse of that text over the section's pins
  (`words`, `seps`, `markers`, `blocks`), with tC4's functions as methods: `parse`, `serialize`,
  `can_drop`, `drop_pin`, `span_end`, `groups`, `keys`, `verses`, `initial_text`, `indent_line`.
  tc-ui and tc-app both run it (AD24).
- **The server derives the section's keys from the displayed tokens** (a Text with `last > verse`
  is the span `9-10`; the verses it covers are tokens too), so the request carries the text and
  the tokens and nothing the server would have to trust. `editor/chapters` gains per verse row
  `format` (`p`, `q1`, `q2` from the previous verse's layout, tC4's `gapMarkerOf`) and `covered`
  (the tokens of the verses a span covers).
- **tc-app builds the values**: a pure function from the buffer and the displayed `VerseValue`s to
  `Option<VerseValue>` per displayed verse, tC4's `saveSection` over the store's value model: a
  group with the same key and the same trimmed body is `None` (writes nothing); a changed body
  replaces `words` only; a changed key list is a join (`Text` with `last`, the rest `Covered`) or
  a break; an empty body is the `___` stub; the layout of a group is the old layout after its
  last verse, with the `\p`/`\q1`/`\q2` line replaced by the typed format (not when the section
  was emptied: #600 changes bodies only).
- **`test/saveSection`** (feature `test-hooks`, like `test/saveVerse`) is the real save through
  the store until C2's `editor/saveSection`: same request shape, C2 renames it. A build without
  it has no section writer: a save is `Unavailable(Writes)`, "Not saved" with Retry.
- **Save only**: the card's Save sends the field's save; blur, the 2 s timer and a pin drop send
  nothing; Cancel discards the field, its entry and its draft. A card whose reply is lost or that
  is saving is read-only, so no keystroke meets an Ack that cannot take it.

## Steps (each one commit, built and tested)

1. **tc-api: `model::section`.** The port of `sectionDraft.js` with the ports of tC4's
   `sectionDraft.test.ts` (all 30), proptests (a placement never changes the words; parse of
   serialize returns the same words and markers; the escape is reversible), `SaveSection`'s wire
   type as `test::SaveSection`, `VerseRow.format` and `covered`.
2. **tc-app: `editor/chapters` fields, the builder, `test/saveSection`.** Tests on the sample
   (join, break, emptied, format, no change, a stale view) and on Psalm 3 and a Genesis 3:14-17
   passage (a no-change save writes nothing; the words of a poem keep their lines), the differential
   test (the UI model and the builder agree on keys and bodies), op counts for a section save.
3. **tc-ui: the card.** `app::section` (pure: the card's place, the Place hints, the pins),
   `views::section_editor` (Type, Place, the bank, the M18 question), `Actions::start_section`, the
   section writer, `VerseMarker` with a catalog label and a drop on release. Host tests for the
   model; `section_editor_saves_only_on_save` and the other J2 section steps as journeys.
4. **Parity.** `en-translate-section-editor` captured; the M18 gallery case's Rust half if
   the harness takes it.
5. **Docs.** APP-DESIGN B4b notes, TC4-ISSUES rows.

## Rules kept

- Nothing typed in a section card is written before Save; Cancel removes the buffer entry and
  the draft; a Save that leaves the section with no words asks first; Keep editing writes nothing.
- A save is a passage batch the store checks for coherence; the displayed tokens are the base, so
  a teammate's change becomes a visible conflict, never a revert.
- Nothing a translator cannot reach is tested.

## Continuation verification (2026-10-20)

Recovered the committed first-round fixes and merged B5b without losing either write
acknowledgement path. The remaining user-visible finding was a typed format on a bare
verse-number line being discarded by Place serialization. Explicit lines now survive;
omitted lines still preserve their stored formats. The new public-API regression fails
without the serializer fix, and the browser journey proves Tab, both mode switches, Save,
keeping an omitted line and removing a format explicitly. PLAN and APP-DESIGN declare the
difference from tC4. API tests: 45; section app tests: 22 including adjacent cases; section
UI tests: 13; parity gate: 48, zero failed rows. Full repository gates, release-path checks,
all four wasm feature checks and all 29 section/verse browser journeys passed.
