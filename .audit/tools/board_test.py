"""Tests for board.py: python3 -I .audit/tools/board_test.py"""
import importlib.util, os, tempfile, time, unittest

spec = importlib.util.spec_from_file_location("board", os.path.join(os.path.dirname(os.path.abspath(__file__)), "board.py"))
board = importlib.util.module_from_spec(spec)
spec.loader.exec_module(board)

FRONT = """---
id: {id}
title: A title
track: A
lane: {lane}
milestone: {milestone}
state: {state}
owner: {owner}
branch: {branch}
worktree: {worktree}
depends: [{depends}]
needs_owner: [{needs}]
proves: [P1]
no_loss: false
size: 1 d
merged: {merged}
updated: {updated}
next: Do step 1.
blocked_by: {blocked_by}
{extra}---

# {id}

## Funnel card

{funnel}

## Steps

{steps}

## Log

{log}
"""

FUNNEL_FULL = """- **Invariants**:
  - INV-X: holds
- **Code**:
  - Types: a newtype
  - One entry point: `f`
  - Removed: nothing
  - Left to checks, and why: none
- **Checks**:
  - `a_test`
  - Proof it can fail: break `f`"""


def card(dir, id, state="todo", owner="none", depends="", needs="", merged="none", blocked_by="none",
         funnel="- **Invariants**:", lane="app", milestone="M1", branch="none", worktree="none",
         updated="2026-10-21", extra="", steps="", log="- 2026-10-21 | roadmap | created | none | made | x"):
    with open(os.path.join(dir, f"{id}.md"), "w", encoding="utf-8") as f:
        f.write(FRONT.format(id=id, state=state, owner=owner, depends=depends, needs=needs,
                             merged=merged, blocked_by=blocked_by, funnel=funnel, lane=lane,
                             milestone=milestone, branch=branch, worktree=worktree, updated=updated,
                             extra=extra, steps=steps, log=log))


class Board(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.units = os.path.join(self.tmp.name, "units")
        self.rulings = os.path.join(self.tmp.name, "rulings")
        self.evidence = os.path.join(self.tmp.name, "evidence")
        os.makedirs(self.units)
        os.makedirs(self.rulings)
        os.makedirs(self.evidence)

    def tearDown(self):
        self.tmp.cleanup()

    def run_board(self, **kw):
        cards, problems = board.load(self.units, use_git=False)
        problems += board.validate(cards, "HEAD", self.rulings, use_git=False, **kw)
        return cards, [p for p in problems if p[0] == "error"], [p for p in problems if p[0] == "warning"]

    def messages(self, problems):
        return [m for _, _, m in problems]

    def test_an_item_is_ready_only_when_its_dependencies_are_done_and_no_owner_question_is_open(self):
        card(self.units, "A-1", state="done", owner="s", merged="abc123", funnel=FUNNEL_FULL,
             log="- 2026-10-21 | s | verify | none | reviewed | x")
        card(self.units, "A-2", depends="A-1")
        card(self.units, "A-3", depends="A-2")
        card(self.units, "A-4", depends="A-1", needs="H5")
        cards, errors, _ = self.run_board()
        self.assertEqual(errors, [])
        ready = board.readiness(cards)
        self.assertEqual(ready["A-2"], "ready")
        self.assertEqual(ready["A-3"], "waits for A-2")
        self.assertEqual(ready["A-4"], "waits for owner: H5")

    def test_an_active_item_without_an_owner_is_an_error_named_after_its_invariant(self):
        card(self.units, "A-1", state="building", funnel=FUNNEL_FULL)
        _, errors, _ = self.run_board()
        self.assertIn("INV-TRACKER-TRUE: state is building but owner is none", self.messages(errors))

    def test_building_without_an_invariant_in_the_funnel_card_is_an_error(self):
        card(self.units, "A-1", state="building", owner="s")
        _, errors, _ = self.run_board()
        self.assertTrue(any("funnel card names no invariant" in m for m in self.messages(errors)), errors)

    def test_building_with_an_empty_funnel_line_is_an_error(self):
        funnel = FUNNEL_FULL.replace("  - Types: a newtype", "  - Types:")
        card(self.units, "A-1", state="building", owner="s", funnel=funnel)
        _, errors, _ = self.run_board()
        self.assertIn("INV-FUNNEL-FIRST: state is building but the funnel card's 'Types' is empty", self.messages(errors))
        # A line whose text continues on the next, deeper-indented line is not empty.
        funnel = FUNNEL_FULL.replace("  - Types: a newtype", "  - Types:\n    a newtype on the next line")
        card(self.units, "A-1", state="building", owner="s", funnel=funnel)
        _, errors, _ = self.run_board()
        self.assertEqual([m for m in self.messages(errors) if "INV-FUNNEL-FIRST" in m], [])

    def test_done_needs_the_merged_commit_and_a_verify_line(self):
        card(self.units, "A-1", state="done", owner="s", funnel=FUNNEL_FULL)
        _, errors, _ = self.run_board()
        self.assertIn("INV-TRACKER-TRUE: state is done but merged is none", self.messages(errors))
        self.assertTrue(any(m.startswith("INV-REVIEWED-BEFORE-MERGE: A-1 is done with no verify line") for m in self.messages(errors)), errors)

    def test_done_before_its_dependency_is_an_error(self):
        card(self.units, "A-1", state="building", owner="s", funnel=FUNNEL_FULL)
        card(self.units, "A-2", state="done", owner="s", merged="abc123", depends="A-1", funnel=FUNNEL_FULL,
             log="- 2026-10-21 | s | verify | none | reviewed | x")
        _, errors, _ = self.run_board()
        self.assertIn("INV-TRACKER-TRUE: done but depends on A-1, which is building", self.messages(errors))

    def test_a_dependency_that_is_blocked_or_parked_is_shown_with_its_reason(self):
        card(self.units, "A-1", state="blocked", owner="s", blocked_by="waits for H9", funnel=FUNNEL_FULL)
        card(self.units, "A-2", state="parked", owner="s", funnel=FUNNEL_FULL)
        card(self.units, "A-3", depends="A-1, A-2")
        cards, _, _ = self.run_board()
        self.assertEqual(board.readiness(cards)["A-3"], "waits for A-1 (blocked: waits for H9), A-2 (parked: Do step 1.)")

    def test_an_active_item_with_an_unmet_dependency_warns_and_a_merging_one_errors(self):
        card(self.units, "A-1")
        card(self.units, "A-2", state="claimed", owner="s", depends="A-1")
        card(self.units, "A-3", state="merging", owner="t", depends="A-1", funnel=FUNNEL_FULL, lane="storage")
        _, errors, warnings = self.run_board()
        self.assertIn("active with A-1 not done: the merge waits", self.messages(warnings))
        self.assertIn("INV-TRACKER-TRUE: state is merging with A-1 not done: the merge cannot land", self.messages(errors))

    def test_a_dependency_on_a_missing_card_and_a_cycle_are_errors(self):
        card(self.units, "A-1", depends="A-2")
        card(self.units, "A-2", depends="A-1, NOPE")
        _, errors, _ = self.run_board()
        messages = self.messages(errors)
        self.assertIn("INV-TRACKER-TRUE: depends on 'NOPE', which has no card", messages)
        self.assertTrue(any("dependency cycle" in m for m in messages), messages)

    def test_unknown_fields_bad_values_and_a_wrong_id_are_errors(self):
        card(self.units, "A-1", state="almost")
        with open(os.path.join(self.units, "A-1.md"), encoding="utf-8") as f:
            text = f.read().replace("title: A title", "title: A title\ncolour: red").replace("id: A-1", "id: A-9")
        with open(os.path.join(self.units, "A-1.md"), "w", encoding="utf-8") as f:
            f.write(text)
        _, errors, _ = self.run_board()
        messages = " | ".join(self.messages(errors))
        self.assertIn("unknown field 'colour'", messages)
        self.assertIn("does not match the file name", messages)
        self.assertIn("state 'almost' is not one of", messages)

    def test_a_blocked_item_must_say_why(self):
        card(self.units, "A-1", state="blocked", owner="s")
        _, errors, _ = self.run_board()
        self.assertIn("INV-TRACKER-TRUE: state is blocked but blocked_by is none", self.messages(errors))

    def test_a_ruling_for_an_open_question_is_a_warning(self):
        card(self.units, "A-1", needs="H5")
        with open(os.path.join(self.rulings, "2026-10-22-H5.md"), "w"):
            pass
        _, errors, warnings = self.run_board()
        self.assertEqual(errors, [])
        self.assertTrue(any("ruling for H5 exists" in m for m in self.messages(warnings)), warnings)

    def test_a_ruling_for_h10_is_not_a_ruling_for_h1(self):
        card(self.units, "A-1", needs="H1")
        with open(os.path.join(self.rulings, "2026-10-22-H10.md"), "w"):
            pass
        _, _, warnings = self.run_board()
        self.assertEqual([m for m in self.messages(warnings) if "ruling" in m], [])

    def test_a_stop_line_for_a_question_that_has_a_ruling_is_a_warning(self):
        card(self.units, "A-1", steps="- Do: STOP until the ruling file exists (H21). Then go on.")
        card(self.units, "A-2", steps="- Do: STOP until the ruling files exist (H18, H22).")
        _, _, warnings = self.run_board()
        self.assertEqual([m for m in self.messages(warnings) if "STOP" in m], [])
        with open(os.path.join(self.rulings, "2026-10-22-H21.md"), "w"):
            pass
        with open(os.path.join(self.rulings, "2026-10-22-H22.md"), "w"):
            pass
        _, _, warnings = self.run_board()
        stops = [m for m in self.messages(warnings) if "STOP" in m]
        self.assertEqual(len(stops), 2, stops)
        self.assertTrue(any("(H21)" in m for m in stops) and any("(H22)" in m for m in stops), stops)

    def test_later_gates_show_on_the_board_and_a_step_past_one_is_an_error(self):
        card(self.units, "S-3", state="building", owner="s", funnel=FUNNEL_FULL, lane="storage")
        card(self.units, "X", extra='later: ["S-3 from step 4", "H12 from step 2"]\n')
        cards, errors, _ = self.run_board()
        self.assertEqual(errors, [])
        self.assertEqual(board.readiness(cards)["X"], "ready to step 1; then S-3 from step 4, H12 from step 2")
        card(self.units, "X", state="building", owner="t", funnel=FUNNEL_FULL,
             extra='later: ["S-3 from step 4", "H12 from step 2"]\n',
             log="- 2026-10-21 | t | claim | none | claimed | x\n- 2026-10-22 | t | step 4 | abc | did it | x")
        _, errors, _ = self.run_board()
        messages = self.messages(errors)
        self.assertIn("INV-TRACKER-TRUE: logged step 4 but S-3 is not done", messages)
        self.assertIn("INV-TRACKER-TRUE: logged step 4 but rulings/ has no H12 file", messages)
        card(self.units, "X", extra='later: ["S-3 step 4"]\n')
        _, errors, _ = self.run_board()
        self.assertTrue(any("is not '<card or H-id> from step <n>'" in m for m in self.messages(errors)), errors)

    def test_two_active_cards_on_one_branch_with_different_owners_is_an_error(self):
        card(self.units, "A-1", state="claimed", owner="s", branch="track-a", worktree="/w/a")
        card(self.units, "A-2", state="claimed", owner="t", branch="track-a", worktree="/w/b", lane="storage")
        _, errors, _ = self.run_board()
        self.assertIn("INV-TRACKER-TRUE: branch track-a is claimed by A-1 (s) and A-2 (t)", self.messages(errors))
        card(self.units, "A-2", state="claimed", owner="s", branch="track-a", worktree="/w/b", lane="storage")
        _, errors, _ = self.run_board()
        self.assertEqual(errors, [])

    def test_two_owners_on_one_lane_is_a_warning(self):
        card(self.units, "A-1", state="claimed", owner="s", lane="app")
        card(self.units, "A-2", state="building", owner="t", lane="app", funnel=FUNNEL_FULL)
        _, _, warnings = self.run_board()
        self.assertIn("lane app has two owners at once: A-1 (s), A-2 (t)", self.messages(warnings))

    def test_updated_must_be_a_date_not_older_than_the_last_log_line_and_a_log_line_must_exist(self):
        card(self.units, "A-1", updated="yesterday")
        card(self.units, "A-2", updated="2026-10-20", log="- 2026-10-22 | s | note | none | later | x")
        card(self.units, "A-3", log="")
        _, errors, warnings = self.run_board()
        self.assertIn("INV-TRACKER-TRUE: updated 'yesterday' is not a date YYYY-MM-DD", self.messages(errors))
        self.assertIn("updated 2026-10-20 is older than the last log line (2026-10-22)", self.messages(warnings))
        self.assertIn("INV-TRACKER-TRUE: has no log line", self.messages(errors))

    def test_done_must_depend_on_every_other_card(self):
        card(self.units, "A-1")
        card(self.units, "A-2")
        card(self.units, "DONE", depends="A-1", milestone="M5", lane="integration")
        _, errors, _ = self.run_board()
        self.assertIn("INV-TRACKER-TRUE: DONE must depend on every other card; missing A-2", self.messages(errors))

    def test_a_roadmap_row_that_disagrees_with_the_card_is_an_error(self):
        card(self.units, "A-1")
        card(self.units, "A-2", depends="A-1")
        card(self.units, "DONE", depends="A-1, A-2", milestone="M5", lane="integration")
        roadmap = os.path.join(self.tmp.name, "ROADMAP.md")
        with open(roadmap, "w", encoding="utf-8") as f:
            f.write("| ID | Item | Lane | Depends on | Size |\n|---|---|---|---|---|\n"
                    "| A-2 | thing | app | A-1, owner H5 | 1 d |\n| DONE | end | integration | every other card | 1 d |\n")
        _, errors, _ = self.run_board(roadmap_path=roadmap)
        self.assertEqual(errors, [])
        with open(roadmap, "w", encoding="utf-8") as f:
            f.write("| ID | Item | Lane | Depends on | Size |\n|---|---|---|---|---|\n| A-2 | thing | app | owner H5 | 1 d |\n")
        _, errors, _ = self.run_board(roadmap_path=roadmap)
        self.assertIn("INV-TRACKER-TRUE: ROADMAP.md's row says it depends on [] but the card says ['A-1']", self.messages(errors))

    def test_a_branch_with_no_commit_for_a_day_warns_and_the_date_shows(self):
        card(self.units, "A-1", state="claimed", owner="s", branch="track-a")
        old = int(time.time()) - 3 * 86400
        calls = []
        def fake_git(*args):
            calls.append(args)
            if args[0] == "rev-parse":
                return 0, "abc"
            if args[0] == "log":
                return 0, str(old)
            return 1, ""
        real = board.git
        board.git = fake_git
        try:
            cards, problems = board.load(self.units, use_git=False)
            problems += board.validate(cards, "HEAD", self.rulings, use_git=True)
        finally:
            board.git = real
        warnings = [m for k, _, m in problems if k == "warning"]
        self.assertTrue(any("track-a last moved 3.0 days ago" in m for m in warnings), warnings)
        self.assertTrue(cards["A-1"]["_last_commit"].startswith(time.strftime("%Y", time.localtime(old))))

    def test_an_active_card_is_read_from_its_branch_when_the_branch_holds_it(self):
        card(self.units, "A-1", state="claimed", owner="s", branch="track-a")
        on_branch = FRONT.format(id="A-1", state="building", owner="s", depends="", needs="", merged="none",
                                 blocked_by="none", funnel=FUNNEL_FULL, lane="app", milestone="M1",
                                 branch="track-a", worktree="none", updated="2026-10-22", extra="", steps="",
                                 log="- 2026-10-22 | s | step 1 | abc | did it | x")
        def fake_git(*args):
            return (0, on_branch) if args[0] == "show" else (1, "")
        real = board.git
        board.git = fake_git
        try:
            cards, _ = board.load(self.units, use_git=True)
        finally:
            board.git = real
        self.assertEqual(cards["A-1"]["state"], "building")
        self.assertEqual(cards["A-1"]["_from_branch"], "track-a")

    def test_a_finished_milestone_without_a_newer_evaluation_warns(self):
        card(self.units, "A-1", state="done", owner="s", merged="abc", milestone="M0", funnel=FUNNEL_FULL,
             updated="2026-10-25", log="- 2026-10-25 | s | verify | none | ok | x")
        card(self.units, "A-2", milestone="M1")
        os.makedirs(os.path.join(self.evidence, "2026-10-21"))
        _, _, warnings = self.run_board(evidence_dir=self.evidence)
        self.assertTrue(any(m.startswith("M0 ended: rerun the evaluation") for m in self.messages(warnings)), warnings)
        os.makedirs(os.path.join(self.evidence, "2026-10-26"))
        _, _, warnings = self.run_board(evidence_dir=self.evidence)
        self.assertEqual([m for m in self.messages(warnings) if "M0 ended" in m], [])

    def test_a_done_sweep_with_no_weekly_line_for_eight_days_warns(self):
        card(self.units, "SWEEP", state="done", owner="s", merged="abc", lane="integration", funnel=FUNNEL_FULL,
             updated="2026-10-21", log="- 2026-10-21 | s | verify | none | ok | x")
        later = time.mktime(time.strptime("2026-11-05", "%Y-%m-%d"))
        _, _, warnings = self.run_board(now=later)
        self.assertTrue(any(m.startswith("SWEEP: last weekly line (2026-10-21) is older than 8 days") for m in self.messages(warnings)), warnings)
        soon = time.mktime(time.strptime("2026-10-25", "%Y-%m-%d"))
        _, _, warnings = self.run_board(now=soon)
        self.assertEqual([m for m in self.messages(warnings) if "SWEEP:" in m], [])

    def test_the_invariant_registry_lists_each_name_with_its_cards(self):
        card(self.units, "A-1", funnel="- INV-ONE: a\n- INV-TWO: b")
        card(self.units, "A-2", funnel="- INV-ONE: a again")
        cards, _, _ = self.run_board()
        reg = board.invariants(cards)
        self.assertEqual(reg["INV-ONE"], {"A-1", "A-2"})
        self.assertEqual(reg["INV-TWO"], {"A-1"})

    def test_the_board_names_an_active_cards_open_owner_question(self):
        card(self.units, "A-1", state="building", owner="s", needs="H5", funnel=FUNNEL_FULL, branch="track-a", worktree="/w/a")
        cards, problems = board.load(self.units, use_git=False)
        text = board.render(cards, problems, "HEAD", self.rulings)
        self.assertIn("Do step 1. waits for owner: H5", text)
        self.assertIn("- A-1 | s | track-a | /w/a | last commit unknown", text)

    def test_the_template_and_the_real_cards_parse(self):
        units = os.path.join(board.REPO, ".audit", "roadmap", "units")
        fields, _, errors = board.parse_card(os.path.join(units, "_TEMPLATE.md"))
        self.assertEqual(errors, [])
        cards, problems = board.load(units, use_git=False)
        roadmap = os.path.join(board.REPO, ".audit", "roadmap")
        problems += board.validate(cards, "HEAD", os.path.join(roadmap, "rulings"), use_git=False,
                                   roadmap_path=os.path.join(roadmap, "ROADMAP.md"),
                                   evidence_dir=os.path.join(roadmap, "evidence"))
        self.assertEqual([p for p in problems if p[0] == "error"], [])


if __name__ == "__main__":
    unittest.main()
