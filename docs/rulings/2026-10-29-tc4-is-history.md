# tC4 4.0 is history; the vision and tC4's design system guide new work

- Asked: 2026-10-29, while rewriting `docs/PLAN.md` for work on top of the finished rebuild:
  does tC4 4.0 still bind new features? Where do the differences from tC4 live? Is the rebuild
  done while P6 (helps from Aquifer) is not proven?
- Answer: 2026-10-29, the owner: "tC4 4.0 is only history. The vision.md and tC4's design system
  are the guide now. The Design system must carry over in to the new Main branch." The
  differences go to their own file. The rebuild is done, and P6 stays open.

## The rule

1. tC4 4.0 no longer binds what the app does. `docs/VISION.md` decides, and tC4's design system
   (its tokens, CSS, fonts, components and the style of its words) is how every screen looks,
   new or old.
2. The design system stays on `main`: `crates/tc-ui/src/ds/`, the CSS and fonts vendored from
   tC4 (`crates/tc-ui/vendored-from-tc4.txt`), and the gallery that draws each component
   (`parity/ds-gallery/`).
3. A screen that came from tC4 may change when the vision calls for it. The change is listed in
   `docs/DIFFERENCES.md` with its issue or ruling. A new screen needs no entry: tC4 has
   nothing to compare it with.
4. The rebuild is done. `docs/PLAN.md` holds the promises P1 to P10 as guarantees every change
   keeps. P6 is open until epic #711 lands.

## Changes

`docs/VISION.md` V8, `AGENTS.md` "Matching tC4", `docs/PLAN.md` (rewritten),
`docs/DIFFERENCES.md` (new, moved from PLAN), and the four checks that read the list of
differences.

## Effect for translators

New features follow what translators need, not what tC4 did. The screens keep looking and
reading the way a tC4 user knows.

## Later the same day: the screenshot comparison is retired

The owner: "P7 still seems to indicate that screenshots need to be taken and compared. That's
been done and is now past." Then: "Retire them." The comparison of the app's screens and
components with tC4's (`screen_record`, `gallery_record`, the baseline and view-gallery record
checks) is removed from the gate. P7 is shown by `crates/tc-ui/tests/vendored_assets.rs`. The
`parity-baseline` CI job keeps its name, since it is a required check, and runs the other Node
checks.
