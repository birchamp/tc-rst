"""Tests for board.py: python3 -I .audit/tools/board_test.py"""
import importlib.util, os, tempfile, textwrap, unittest

spec = importlib.util.spec_from_file_location("board", os.path.join(os.path.dirname(os.path.abspath(__file__)), "board.py"))
board = importlib.util.module_from_spec(spec)
spec.loader.exec_module(board)

FRONT = """---
id: {id}
title: A title
track: A
lane: app
milestone: M1
state: {state}
owner: {owner}
branch: none
worktree: none
depends: [{depends}]
needs_owner: [{needs}]
proves: [P1]
no_loss: false
size: 1 d
merged: {merged}
updated: 2026-10-21
next: Do step 1.
blocked_by: {blocked_by}
---

# {id}

## Funnel card

{funnel}

## Log
"""


def card(dir, id, state="todo", owner="none", depends="", needs="", merged="none", blocked_by="none", funnel="- **Invariants**:"):
    with open(os.path.join(dir, f"{id}.md"), "w", encoding="utf-8") as f:
        f.write(FRONT.format(id=id, state=state, owner=owner, depends=depends, needs=needs,
                             merged=merged, blocked_by=blocked_by, funnel=funnel))


class Board(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.units = os.path.join(self.tmp.name, "units")
        self.rulings = os.path.join(self.tmp.name, "rulings")
        os.makedirs(self.units)
        os.makedirs(self.rulings)

    def tearDown(self):
        self.tmp.cleanup()

    def run_board(self):
        cards, problems = board.load(self.units)
        problems += board.validate(cards, "HEAD", self.rulings, use_git=False)
        return cards, [p for p in problems if p[0] == "error"], [p for p in problems if p[0] == "warning"]

    def test_an_item_is_ready_only_when_its_dependencies_are_done_and_no_owner_question_is_open(self):
        card(self.units, "A-1", state="done", owner="s", merged="abc123", funnel="- INV-X: holds")
        card(self.units, "A-2", depends="A-1")
        card(self.units, "A-3", depends="A-2")
        card(self.units, "A-4", depends="A-1", needs="H5")
        cards, errors, _ = self.run_board()
        self.assertEqual(errors, [])
        ready = board.readiness(cards)
        self.assertEqual(ready["A-2"], "ready")
        self.assertEqual(ready["A-3"], "waits for A-2")
        self.assertEqual(ready["A-4"], "waits for owner: H5")

    def test_an_active_item_without_an_owner_is_an_error(self):
        card(self.units, "A-1", state="building", funnel="- INV-X: holds")
        _, errors, _ = self.run_board()
        self.assertIn("state is building but owner is none", [m for _, _, m in errors])

    def test_building_without_an_invariant_in_the_funnel_card_is_an_error(self):
        card(self.units, "A-1", state="building", owner="s")
        _, errors, _ = self.run_board()
        self.assertTrue(any("funnel card names no invariant" in m for _, _, m in errors), errors)

    def test_done_needs_the_merged_commit(self):
        card(self.units, "A-1", state="done", owner="s", funnel="- INV-X: holds")
        _, errors, _ = self.run_board()
        self.assertIn("state is done but merged is none", [m for _, _, m in errors])

    def test_a_dependency_on_a_missing_card_and_a_cycle_are_errors(self):
        card(self.units, "A-1", depends="A-2")
        card(self.units, "A-2", depends="A-1, NOPE")
        _, errors, _ = self.run_board()
        messages = [m for _, _, m in errors]
        self.assertIn("depends on 'NOPE', which has no card", messages)
        self.assertTrue(any(m.startswith("dependency cycle") for m in messages), messages)

    def test_unknown_fields_bad_values_and_a_wrong_id_are_errors(self):
        card(self.units, "A-1", state="almost")
        with open(os.path.join(self.units, "A-1.md"), encoding="utf-8") as f:
            text = f.read().replace("title: A title", "title: A title\ncolour: red").replace("id: A-1", "id: A-9")
        with open(os.path.join(self.units, "A-1.md"), "w", encoding="utf-8") as f:
            f.write(text)
        _, errors, _ = self.run_board()
        messages = " | ".join(m for _, _, m in errors)
        self.assertIn("unknown field 'colour'", messages)
        self.assertIn("does not match the file name", messages)
        self.assertIn("state 'almost' is not one of", messages)

    def test_a_blocked_item_must_say_why(self):
        card(self.units, "A-1", state="blocked", owner="s")
        _, errors, _ = self.run_board()
        self.assertIn("state is blocked but blocked_by is none", [m for _, _, m in errors])

    def test_a_ruling_for_an_open_question_is_a_warning(self):
        card(self.units, "A-1", needs="H5")
        with open(os.path.join(self.rulings, "2026-10-22-H5.md"), "w"):
            pass
        _, errors, warnings = self.run_board()
        self.assertEqual(errors, [])
        self.assertTrue(any("ruling for H5 exists" in m for _, _, m in warnings), warnings)

    def test_a_ruling_for_h10_is_not_a_ruling_for_h1(self):
        card(self.units, "A-1", needs="H1")
        with open(os.path.join(self.rulings, "2026-10-22-H10.md"), "w"):
            pass
        _, _, warnings = self.run_board()
        self.assertEqual([m for _, _, m in warnings if "ruling" in m], [])

    def test_the_template_and_the_real_cards_parse(self):
        units = os.path.join(board.REPO, ".audit", "roadmap", "units")
        fields, _, errors = board.parse_card(os.path.join(units, "_TEMPLATE.md"))
        self.assertEqual(errors, [])
        cards, problems = board.load(units)
        problems += board.validate(cards, "HEAD", os.path.join(board.REPO, ".audit", "roadmap", "rulings"), use_git=False)
        self.assertEqual([p for p in problems if p[0] == "error"], [])


if __name__ == "__main__":
    unittest.main()
