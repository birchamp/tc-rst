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
  - `stay_keeps_the_click_on_the_page`: a cut connection; after Stay, Retry saves the click.
  - Other ways out (a tab, a book, another project, quitting) are unchanged: the ruling is
    Home's. They still hold while a click is refused, and Home now always offers a way out.
  - Proof it can fail: one planted break per journey (log).
- **Prose**: `docs/PLAN.md`'s #801/#807 difference names the question. Text: `align.leave.*`
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
