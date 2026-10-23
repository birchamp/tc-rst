# Plan for design unit B5a: Check, picker and rail

Worktree `.claude/worktrees/lane-b5a`, branch `lane-b5a`, cut from `claude/focused-ride-5uzj7p` at
`d250a89`. Row (docs/APP-DESIGN.md §10, phase 1): preflight; picker progress; `CheckListCache` with
per-row `ViewSeq`; `check/open { since }` and `check/rows { since }` (A-12); `model::rail` (mask,
one-pass groups (A-13), patch); the virtual list with two heights and a mounted tab-stop row
(A-24); M11 view (the guided fix). Tests: `check_open_cached_derives_nothing`;
`check_open_cached_sends_changed_rows_only`; the rail differential test against
`tc_check::rail::groups`; `rail_groups_visit_each_row_once`; check-keyboard; rail perf. Done when:
`check-picker`, `en-check-picker`, `guided-fix` are in the report, with the `check-empty` gallery
cases. No Q-n or R-n waits.

tC4 issues for B5a (TC4-ISSUES): #764 visible bar at low progress (declared difference), #518 and
#513 resume into a tool (no clickable picker, one open, a tool chosen meanwhile stays), #42 the
keyboard pass (picker, guided fix, rail), #501 opening Check writes nothing. #769 (fold, By
figure) is a "reference change": matched when the reference moves (C44's vendoring brings its
keys), so the rail stays at d310bf3's By category. The target-words follow-up (TC4-ISSUES, "B5a,
C4") belongs to the screen that draws target words, B5b.

## Steps (each one commit, built and tested)

1. **tc-api.** `check/picker` (preflight per tool with its progress, the Align card, the old-twl
   pin), `check/open { since }`, `check/rows { since }`, `resources/fixState` (M11's view); the
   views (`PickerView`, `ToolCard`, `CheckSessionView` with `RailRows::{Full, Since}`, `RowsView`,
   `FixView`); `KeptPlace.tool` for the resume; `model::rail`: `Filter`, `SortMode`, `mask`,
   `groups` (one pass over the masked rows, buckets by term index or category), `patch`; goldens.
2. **tc-app.** `check.rs`: preflight from `tc_resources::resolve::preflight_tool_book` (tC4's
   `online = true`, D88); the derive from the book's Door43 helps (B3's book cache, Door43 even
   when the reading helps are Aquifer, Q33); decisions placed with `tc_check::session::open` over
   the fold's projection; `CheckListCache` per session keyed by (list, resolution, versification,
   scope), each row stamped with the `ViewSeq` that last changed it; a cached open derives
   nothing and sends only rows newer than the UI's view. Tests through `App::request` on the
   parity scenarios, the differential rail test on Titus and Psalms, count rows on Psalms.
3. **tc-ui.** `app::check` (pure): screen state with the resume rule, CTA and next-line rules,
   the rail keys (port of `railKeyDown`), the virtual list (two heights, offsets, the mounted
   tab-stop row, threshold 400 rows); views `check/picker`, `check/rail`, `check/session`, the
   empty states and `modals/guided_fix`; the shell draws Check for a Bible project.
4. **Parity.** `units.json` gains B5a; capture the three screens and the `check-empty` cases;
   the check-keyboard journey (picker, guided fix, rail); report reviewed, residuals written.
5. **Docs.** B5a's notes in the row (D-n), TC4-ISSUES rows for B5a.

## Rules kept

- Nothing in B5a writes the project: the picker, a tool's open and the guided fix's view read only
  (#501's test proves it on the fold and the files).
- A count the server could not compute is stated, never shown as 0 (tC4's `progressError`).
- A check dropped by the project's numbering is counted beside the progress it qualifies (#15).
