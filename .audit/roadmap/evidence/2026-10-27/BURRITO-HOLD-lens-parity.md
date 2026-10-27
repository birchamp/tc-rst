# BURRITO-HOLD: browser and parity lens

Reviewer: independent (browser and parity lens), 2026-10-27. Diff: `git diff 7285d2c0..eee0a0fe`.
No browser run: nothing a translator sees changed (below), so the lens rule needs none.

## What a translator sees

- Code changed: one arm of `Writer::metadata_entries` (`crates/tc-store/src/materialize.rs:2021-2039`)
  and a new reader `listed_in_metadata` (`:1922-1933`). Both decide only the md5 and size written into
  the folder's `metadata.json` for a path in `Writing` whose file cannot be read. No notice is raised,
  dropped or reworded; the arm emits nothing.
- Notices checked: `OutsideEditHeld` (`handle.rs:508,560`), `FileUnreadable` (`open.rs:959`,
  `classify.rs:587`, `writer.rs:447`), `MaterializeBlocked` (`writer.rs:402`), `OutsideEditUnreadable`
  (`outside.rs:656`), and their views in `crates/tc-app/src/conflicts.rs:308-327`. None reads the
  ingredient list; none is reached from `metadata_entries`.
- tc-app reads `metadata.json` only for its free fields (Home card name, languages, direction:
  `home.rs:171-187`, `direction.rs:24`, `upgrades.rs:186`); no tc-app or tc-api code reads
  `ingredients`/`checksum`. So no screen depends on the listing of a held file.
- The exchange (USB) file and Door43 push: built from HEAD after a checkpoint, whose tree comes from
  the projection (`build_tree`), not from the folder's list. The diff changes no export code; the new
  test `exchange::an_export_while_a_book_file_is_undecided_is_a_valid_burrito` pins it.
- Journeys: none in `parity/app/journeys` drives a held book file inside an interrupted write group
  (it needs a kill mid-group plus another program locking the file, which the browser harness cannot
  stage). `j07-export.test.mjs` uses the export endpoint, whose behavior is unchanged.
  `conflict-kinds.test.mjs` mentions "held" only for a decision file's comment.
- tC4 parity: tC4 4.0 writes no Scripture Burrito `metadata.json` ingredient list of this kind, so
  there is no tC4 behavior to match and nothing to declare in `docs/APP-DESIGN.md`.

## FUNNEL §8 questions

1. **Code.** The fix is a deletion (the "list the bytes before the write" fallback) plus one rule at
   the single entry point that builds the list. No type would carry "unknown bytes" more cheaply:
   `Disk::Unreadable` is already the enum that says it. Adequate.
2. **Checks.** Five tests named after the invariant's cases exist (`tests/store/groups.rs`,
   `tests/sync/exchange.rs`); the card log records each failing with its guard planted away. They sit
   in the tc-store and tc-sync test binaries that CI runs. I did not rerun them (outside this lens).
3. **Reach.** Reachable on Windows: a kill (power loss, crash) between a book file's rename and the
   `metadata.json` rewrite, then an editor, antivirus or cloud placeholder holding the file. Not
   reachable through the browser, which is why no journey covers it.
4. **Siblings.** Within this lens: no other screen or export reads the folder's list. The card checks
   commits and exchange files; Door43 push uses the same commits.
5. **No loss.** No text, alignment, decision or comment is touched: only the metadata bytes of an
   undecided file. Nothing visible; the loss lens owns the crash/model-test question.
6. **Prose and Human.** The new code comment (`materialize.rs:2021-2024`) gives the reason (either
   version may be there, so make no new claim), not a narration. Test doc comments state the scenario,
   which is their job. No owner question: nothing a translator notices. STORAGE I7, §5.10 M2 and L153
   match the code (an undecided path keeps the entry `metadata.json` has now; heals at the first read).

## Findings

Blockers: none. Majors: none.

Minors:

- `docs/STORAGE.md:2044-2046` (M2) and `:86` (I7) say an undecided path "keeps the entry
  `metadata.json` has for it now", but do not say what happens when it has none (no entry, or
  `metadata.json` itself held or unparseable): `materialize.rs:2035` then omits the path. Before the
  diff such a path with `old: Some` was listed as `old`. Omitting makes no false claim, so it fits I7,
  but the text should say it. No translator-visible effect.
- `docs/STORAGE.md:86`: I7 names the card ("(BURRITO-HOLD)") inside the invariant table; elsewhere
  the design cites sections, not card ids. Cosmetic.
