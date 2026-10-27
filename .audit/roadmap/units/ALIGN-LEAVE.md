---
id: ALIGN-LEAVE
title: Home asks before it leaves an aligner click behind
track: A
lane: app
milestone: M3
state: verifying
owner: E1d lane (Claude Code 07ffe14c)
branch: track-ALIGN-LEAVE
worktree: .worktrees/track-ALIGN-LEAVE
depends: [E1d]
needs_owner: []
proves: [P2]
no_loss: true
size: 0.5 d (basis: one wait loop, one question, four journeys)
merged: none
updated: 2026-10-26
next: Three lenses (loss first), then the touched gate and the hand-over to the integration lane.
blocked_by: none
---

# ALIGN-LEAVE: Home asks before it leaves an aligner click behind

## Goal

A click in the aligner that has not reached the server lives only in the page. Home used to
keep the person in the aligner with no way out when such a click was refused. Now Home waits
while the click is on its way, sends it again when a send fails, and leaves as soon as it
saves. Only a click lost for sure asks: Stay, or Leave without it (owner ruling,
`rulings/2026-10-26-ALIGN-LEAVE.md`).

## Why it matters to a translator

Home never traps them, never asks about a click that saves a moment later, and never drops a
click without asking.

## Funnel card

- **Invariants**:
  - INV-HOME-ASKS-ONLY-WHEN-A-CLICK-IS-LOST: Home asks about an unsaved aligner click only when
    the server refused it for good or it is still unsaved after `LEAVE_WAIT_MS` (3 s); Leave
    without it drops only those clicks, and Stay keeps them on the page.
- **Code**: `EditQueue::on_leave` (pure, `crates/tc-ui/src/app/align.rs`) decides Leave, Wait,
  Retry or Ask; `settle_align_clicks` and the question (`crates/tc-ui/src/app/web/align.rs`,
  `views/align/session.rs`) run it before Home's gate (`app/web/leave.rs`). The limit: a
  click's painted round trip is p50 31 ms, p95 42 ms in a debug build
  (`align_click_round_trip_p95`, 2026-10-26); a retry is two round trips, sent every 250 ms;
  3 s is about 70 slow clicks, so an unsaved click then is stuck, and the wait stays short.
- **Checks** (journeys in `parity/app/journeys/align.test.mjs`; unit
  `home_asks_only_once_a_click_is_lost_for_sure` in `crates/tc-ui/src/app/align.rs`):
  - `a_click_that_saves_within_the_wait_leaves_with_no_question`: the click is held 1 s; Home
    leaves with no question, and the click is saved.
  - `a_click_refused_for_good_asks_and_leave_without_it_goes_home`: asks within 2 s; Leave
    without it goes Home.
  - `a_stuck_click_asks_after_the_limit_and_stay_keeps_it`: asks only after the limit; Stay
    stays, and the held click saves when it is let through.
  - `a_click_that_saves_while_home_asks_closes_the_question_and_goes_home`: the held click is
    let through while the question shows; the question closes and Home goes on.
  - `stay_keeps_the_click_on_the_page`: a cut connection; Escape is Stay, and Retry then
    saves the click.
  - `a_click_sent_again_after_a_cut_leaves_with_no_question`: the connection comes back after
    1 s; Home sends the click again and leaves with no question.
  - Focus: the question takes focus on Stay (in the stuck-click journey).
  - Other ways out (a tab, a book, another project, quitting) are unchanged: the ruling is
    Home's. They still hold while a click is refused, and Home now always offers a way out.
  - Proof it can fail: one planted break per journey (log).
- **Prose**: `docs/PLAN.md` declares the wait and the question (tC4 does neither). Text: `align.leave.*`
  in `crates/tc-ui/assets/i18n-rebuild/en.json`, pending in all nine languages.
- **Human**: the ruling above.

## Steps

### Step 1: Wait, then ask

- Done when: the four journeys and the unit test pass, each journey fails under its planted
  break, and `scripts/gates.sh --touched` exits 0.

## Log

- 2026-10-26 19:58 EDT | E1d lane (Claude Code 07ffe14c) | created | none | Created at the orchestrator's request after the owner's ruling on E1d's follow-up; branch off track-E1d | rulings/2026-10-26-ALIGN-LEAVE.md
- 2026-10-26 19:58 EDT | E1d lane (Claude Code 07ffe14c) | ruling | none | The owner: "1, but wait until you're sure that the question needs to be asked. I don't want them to be asked if it's going to be saved in a second or two." | rulings/2026-10-26-ALIGN-LEAVE.md
- 2026-10-26 20:41 EDT | E1d lane (Claude Code 07ffe14c) | step 1 | 8c6af25a | Home waits, sends a failed click again every 250 ms, and asks only once it is lost; the 3 s limit is from the measured round trip (p50 31 ms, p95 42 ms, debug). Breaks, each failing with INV-HOME-ASKS-ONLY-WHEN-A-CLICK-IS-LOST: Ask on any pending click fails `a_click_that_saves_within_the_wait_leaves_with_no_question` (no question for a click that saved); a refusal that waits fails `a_click_refused_for_good_asks_and_leave_without_it_goes_home` (asks at once); no limit fails `a_stuck_click_asks_after_the_limit_and_stay_keeps_it` (a click stuck past the limit asks); Stay that drops the clicks fails `stay_keeps_the_click_on_the_page` (Retry saves it). The whole align journey file passes (20) | node --test parity/app/journeys/align.test.mjs
- 2026-10-26 20:48 EDT | loss lens (Claude Code subagent of 07ffe14c) | verify | 03821376 | No blocker, no major. Leave drops only the page's unsaved clicks; the server's links are flushed after, and a late in-flight edit can only add. Retries resend the same request id, which the reply cache runs once. The question is mounted on every aligner screen state. Minors: the question did not close when the click saved meanwhile (fixed); the drop comes before the gate, so a standing field failure keeps the person with the click gone (they chose to drop it; kept); a second Home press orphans the first wait (harmless). Six questions: code pure `on_leave` and one entry in `leave()`; checks four journeys and a unit, each broken once; reach yes; siblings: other leave paths unchanged, now said on the card; no loss; prose fine | code reading
- 2026-10-26 20:48 EDT | E1d lane (Claude Code 07ffe14c) | fix | see branch | A click that saves while Home asks closes the question and Home goes on (`align_edits_changed`); new journey `a_click_that_saves_while_home_asks_closes_the_question_and_goes_home` fails without it (INV-HOME-ASKS-ONLY-WHEN-A-CLICK-IS-LOST: a click that saves while asked is not lost, so Home goes on). The card says why the other leave paths are unchanged | node --test parity/app/journeys/align.test.mjs (21 pass)
- 2026-10-26 21:04 EDT | races and protocol lens (Claude Code subagent of 07ffe14c) | verify | 0ae2b378 | No blocker, no major. Minors: a second Home press started a second wait that could ask again after Stay, or take the person Home against it (fixed: only the newest press waits); a restart during the wait ran the gate against the new run (fixed: no project, no gate); a click made at 2.9 s can be asked about while it saves (the question closes on the reply; kept); no journey for the retry path (added); the within-wait journey did not prove Home waited (added). Six questions: code `LeaveWait` good, a `Latest` token for the presses; checks journeys plus unit; reach yes; siblings: close and open of the aligner drain without asking, outside this card; no loss; prose fine | code reading
- 2026-10-26 21:04 EDT | browser and parity lens (Claude Code subagent of 07ffe14c) | verify | 0ae2b378 | 20 of 21 journeys pass (`align_by_keyboard_only` hung once under load, then passed alone; outside this diff). Majors: focus stayed behind the scrim when the question opened (a Layer whose panel mounts after `open` turned true, `ds/primitives/layer.rs`); the text said everything else stays though Leave drops every click after the stuck one; PLAN did not declare the wait and the question. Minors: keys out of order in en.json; no journey for Escape or focus; Home shows no sign of the wait (kept: the save indicator shows it). Six questions: code good; checks lacked focus and Escape; reach yes; siblings: the Layer fix reaches every late-mounting layer; no loss; prose needed the difference | probe in the scratchpad, node --test align
- 2026-10-26 21:04 EDT | E1d lane (Claude Code 07ffe14c) | fix | see branch | Only the newest Home press waits (`SETTLES`); a restart during the wait ends it; the Layer moves focus into a panel that mounts late; the question's words say the last words placed are lost; PLAN declares the difference; keys sorted. New journey `a_click_sent_again_after_a_cut_leaves_with_no_question` fails when a failed send asks at once; the focus check fails with the Layer fix removed (`hung: no focus moves into the question`). Align journeys: 22 pass | node --test parity/app/journeys/align.test.mjs
- 2026-10-26 21:54 EDT | E1d lane (Claude Code 07ffe14c) | fix | see branch | The whole journey suite found two faults of the last fixes: the Layer's late focus also moved focus in layers that hold it on purpose (About, app language, check keyboard, help: 13 journeys), so it now runs only when the open found no panel, once per open; the restart check stopped Home from the error screen, where no project is open (`a_panic_reload_keeps_the_top_bar_and_back_to_home_gates`), so it now applies only when a project was open before the wait. Suite: 294 of 301 pass, 5 skipped (the view gallery needs TC4_ROOT), and the 2 failures pass after the fix (shell and align: 30; guided-fix: 3, with Playwright's node_modules linked) | node --test parity/app/journeys/*.test.mjs
