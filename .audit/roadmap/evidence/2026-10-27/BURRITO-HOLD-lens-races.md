# BURRITO-HOLD: races and protocol lens

Reviewer: independent verifier, races and protocol lens. 2026-10-27.
Change: `git diff 7285d2c0..eee0a0fe` (`Writer::metadata_entries`, new `Writer::listed_in_metadata`,
`crates/tc-store/src/materialize.rs:1923`, `:2034`). Tests run:
`tc-heavy cargo nextest run -p tc-store --test store held_book_file book_file_is_undecided cannot_read_a_book_file a_group_keeps_the_intent`:
6 passed. No probe test left behind.

## The six questions (FUNNEL §8)

1. **Code.** The card says no type, and that is right: the undecided state already exists
   (`State::Writing` plus `Disk::Unreadable`). The fix is one entry point (`metadata_entries`)
   and it deletes the `old`-placement fallback. A tighter structural form exists: take the
   entry from the `metadata.json` bytes the group already read at M1
   (`materialize.rs:720`, `MetadataRead::Bytes`) instead of reading the file again. That would
   make "the list and the M1 decision see the same bytes" true by construction, not by the M6
   last look (minor m1). It is not a major: the last look already closes the race (below).
2. **Checks.** Five tests named after the invariant, plus the pre-existing repro, all pass
   (0.4 to 0.6 s each, deterministic fault fs). The card's log records each failing with its
   guard planted. No test covers a concurrent outside write to `metadata.json` between M1 and
   the listing; I found no hook to inject one, and the code path is covered by the existing
   M6 last-look tests (minor m2, coverage only).
3. **Reach.** Every scenario I keep is one a translator reaches: kill or power cut mid-group,
   then Paratext / an antivirus scan / a cloud placeholder holding `TIT.usfm`, or a cloud-sync
   client replacing `metadata.json`. Dropped as unreachable: an outside program writing
   `metadata.json` to B and back to A inside the milliseconds between M1 and M6 (ABA); a person
   hand-editing an ingredient checksum in `metadata.json` while that book file is held and
   undecided (would make `shown_metadata` in `outside.rs:333` see the edit as unchanged and
   undercount `derived_fields`).
4. **Siblings.** Searched `State::Writing { old: Some … }` and `old.size` across
   `crates/tc-store/src`: the deleted fallback was the only listing that used the `old`
   placement. `ledger.rs:363,522` use it for decisions, not for listing. Only `materialize.rs`
   writes `metadata.json`, on the writer thread; the project lock (`LOCK`) keeps a second
   session out, so there is no second app writer.
5. **No loss.** This change writes no translator data; it only chooses what `metadata.json`
   claims. Race walk-through below: no path lets the app adopt an outside program's
   `metadata.json` entry without a durable copy, and no path writes a list built from bytes a
   concurrent write replaced.
6. **Prose and Human.** The new comment at `materialize.rs:2021-2024` states a reason (either
   bytes may be there, so no new claim until a read). The doc comment on `listed_in_metadata`
   is a one-line what, acceptable for a helper. No owner question: nothing a translator sees
   changes.

## Race walk-through (the lens questions)

Order in one group (`run_group`): `metadata.json` read at `:720` (bytes A) → listing
`build_group_metadata` at `:878` (calls `listed_in_metadata`, a second read: A') → ledger
decides A at `:882` (`look_at_metadata`) → M3 to M5 → in the skipped case the list is built a
third time at `:1137` (A'') → last look `last_look_at_metadata` at `:1308` compares the disk
with A and with the new md5 → rename.

- **Outside write to `metadata.json` between `:720` and the listing** (A → B, B's entry for an
  undecided `TIT.usfm` adopted into the new list): at the last look the disk holds B, which is
  neither A nor the new bytes, so `keep_edited_metadata` copies B with a notice and the rename
  is refused (`:1314-1322`). The list containing B's entry is never written. Safe.
- **Outside write before the group** (cloud sync put another computer's `metadata.json`):
  A is foreign at `:882`. Record/Hold: `taken != Accepted`, the group writes nothing. Keep and
  write: the copy is made first (§5.10 table step 1), then the list that may carry B's entry
  for an undecided path is written. Copy kept; the entry is what the folder already claimed,
  which is the ruling. Safe. Note the listing at `:878` runs before the decision at `:882`, but
  nothing is written between them.
- **`metadata.json` becomes unreadable mid-group**: `listed_in_metadata` returns None, the path
  drops from the list (the build may then fail and the group waits), and the last look notes
  `Unreadable` and refuses. Nothing written. Safe.
- **`TIT.usfm` becomes readable mid-group**: the next listing (any book's group: the listing
  walks every manifest path) reads it and lists its real bytes. Heals. Safe.
- **`TIT.usfm` becomes unreadable mid-group, normal state**: M4's last look refuses with
  `WriteFailed` → state `Placed{old}`, so `e.listed()` is Some and the new branch is not taken.
  A rename that went through is `Placed{new}`. The new branch is reached only after a kill or a
  `WrittenNotDurable` error leaves `Writing`.
- **Retry / duplicate checkpoint / watcher and focus re-reads**: an unreadable path is `Wait`
  in M1 and keeps `Writing{old,new}`; no second `WriteBegun` can name new bytes without a read,
  so the entry carried forward each group stays the one the folder claimed. Idempotent.
- **Sync or export mid-save/checkpoint**: commits and exchange files come from the projection
  (`build_tree`; `tc_sync::exchange::export` after a checkpoint), never from this list; proof
  (1) and (3) cover it.
- **Ledger record of `metadata.json`**: `listed_in_metadata` neither consults nor updates the
  ledger. Every write of the list still passes `look_at_metadata` (decided on A) and the last
  look (A or new), so a list built from any other bytes is refused. Consistent.

**Cost** (docs/PERFORMANCE.md): one read and one `serde_json::Value` parse of `metadata.json`
per undecided unreadable path, up to three times per group (`:878`, `:896` with carried files,
`:1137` when a write was skipped). Every group already reads and hashes the whole
`metadata.json` at M1, so this is the same order and only while a path is undecided (after a
kill, until the file reads). Acceptable. The extra reads are not counted in
`counters.derived_read`, unlike the M1 read at `:1265` (part of m1).

## Findings

### Blockers

None.

### Majors

None.

### Minors

- **m1. The listing reads `metadata.json` again instead of using the bytes M1 read and decided.**
  `crates/tc-store/src/materialize.rs:1923-1933`, called from `:2034`; M1 read at `:720`.
  Scenario: a translator's project after a power cut mid-save of Titus; Paratext holds
  `TIT.usfm`; OneDrive replaces `metadata.json` during the 3 s materialization. Today the M6 last
  look refuses the write and keeps a copy, so no wrong list lands; but correctness rests on the
  last look, and each group may parse the file up to three times while a path is undecided.
  Fix: parse the entry once from `MetadataRead::Bytes` (or cache it per group) and pass it into
  `metadata_entries`; count the read.
- **m2. No test injects an outside `metadata.json` write between M1 and the listing while a
  path is undecided.** `crates/tc-store/tests/store/groups.rs` (new tests at the end). Same
  scenario as m1. The protection is generic (M6 last look) and tested elsewhere; a test here
  would pin that the adopted entry never lands without a copy. Needs a hook between M1 and M2
  (none in `hooks.rs` today), so a minor.

Count: 0 blockers, 0 majors, 2 minors.
