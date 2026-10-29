# Ruling: tests prove behavior, not speed; do not optimize ahead of the user

- Asked: 2026-10-23, by the owner, in conversation with the roadmap session, after timing
  flakes on loaded CI runners held every merge.
- Answer, in the owner's words: "it seems like maybe you're pre-optimizing. I've found that
  often we don't know what the real bottlenecks are until the system is built and integrated.
  So we can spend a lot of time optimizing something that isn't a problem for the user. Be
  careful not to slow down development just so we can run the perfect test so we can get the
  perfect timings. This is a trade off and judgement call because at some point waiting 6
  minutes for something makes no sense to the user and it has to be optimized, but the
  difference between 6s and 10s is minimal as long as it doesn't interrupt the user's actions."

## The rule

1. **A test asserts what the app does, never how fast.** No test in the gate list fails on a
   wall-clock limit. Where a test must wait for something to happen, it waits for the event with
   a limit that only catches a hang (on the order of a minute, not seconds), and its failure says
   the step hung, not that it was slow.
2. **Speed is judged only by the budgets** of `docs/PERFORMANCE.md`, measured in release builds
   on the reference laptop (cards P and G2), at the existing 3x tolerance in CI. A budget matters
   when a wait interrupts the translator's action; a few seconds more on a background task does
   not.
3. **No optimization work ahead of a measured problem.** A card does speed work only when a
   budget row misses on the reference laptop, or a translator-visible action waits long enough
   to interrupt them. Op-count tests stay: they count operations, not time.
4. **CHK-4 shrinks to the flakes.** It raises or removes the short waits in the tests that have
   flaked under load, and nothing more until a real flake appears. Virtual time, fixed random
   seeds and per-binary time budgets are not built now.

## Effect for translators

None directly; it keeps the team building features instead of tuning tests.

## Unblocks and changes

Cards CHK-4 (cut to the flaky waits), P, G2, FUNNEL.md §5 "Deterministic" and "Fast" rows
(a check must not depend on wall-clock speed; "fast" means the suite stays usable, not a
per-binary budget), and every card's test steps.
