---
id: FEEDBACK-PATHS
title: A Feedback report never carries the home folder or the account name
track: A
lane: app
milestone: M5
state: gating
owner: tester findings lane (Claude Code 76125a0f)
branch: track-REL1-tester
worktree: .worktrees/track-REL1-tester
depends: []
needs_owner: []
proves: []
no_loss: false
size: 0.5 d (basis: one pure function at the one place the attachment is composed)
merged: none
updated: 2026-10-27
next: scripts/gates.sh --touched with main merged in, then push and hand to the integration lane.
blocked_by: none
---

# FEEDBACK-PATHS: A Feedback report never carries the home folder or the account name

## Goal

A 4.1.0-preview tester found full paths from their computer in the Feedback attachment. The first
launch's report names the projects folder under the home folder, and an error's reason or
facts can name any path. `feedback/compose` now passes the attachment through
`tc_app::feedback::redact`: the home folder becomes `~`, and the account name (the home
folder's last segment) becomes `<user>` wherever it is a whole path segment, with either
separator, JSON's doubled backslashes, and ASCII case ignored. The reports kept on the
computer (`reports.json`, the Inspector) stay whole.

## Why it matters to a translator

A report goes by email to the help desk. A home folder names the person, and in some places
naming a translator puts them at risk.

## Funnel card

- **Invariants**:
  - INV-FEEDBACK-NO-HOME: no attachment `feedback/compose` answers contains the home folder.
- **Code**: `feedback::redact`; `Inner::home_folder` from `config::home_dir`; the `Compose`
  handler. The version line and the shell's mail add no path.
- **Checks**: `the_attachment_carries_no_home_folder_and_no_account_name` (fails on main
  020e0ddf: the first-launch report shows the home path) and
  `redact_takes_out_a_home_folder_on_each_system` (macOS, Windows, Linux forms) in
  `crates/tc-app/tests/app/feedback.rs`.
- **Prose**: declared difference `feedback-attachment-redacts-the-home-folder` in
  `parity/app/residuals.json`: tC4 4.0 sends its report's paths whole.
- **Human**: the owner is told of the declared difference.
- Left: a name inside free text that is not a path segment ("OneDrive - Ana Lopez") stays;
  a one-letter account name is not taken out.

## Steps

### Step 1: redact and its tests
### Step 2: One review; `scripts/gates.sh --touched` with main merged in

## Gates before merge

`scripts/gates.sh --touched` with main merged in; one `verify` line.

## References

- Tester report on 4.1.0-preview (relayed by the audit session, 2026-10-27); tC4
  `src/data/feedback.ts` `attachmentText`.

## Log

- 2026-10-27 13:00 EDT | tester findings lane (Claude Code 76125a0f) | created | none | Reproduced: a fresh install's attachment held the projects folder under the home folder | crates/tc-app/tests/app/feedback.rs the_attachment_carries_no_home_folder_and_no_account_name
- 2026-10-27 13:05 EDT | tester findings lane (Claude Code 76125a0f) | claim | none | Claimed with the fix on track-REL1-tester | none
- 2026-10-27 12:45 EDT | tester findings lane (Claude Code 76125a0f) | verify | 1943b005 | One review (general-purpose agent, FUNNEL §8): pass with minors. Windows, JSON-escaped, \\?\ and macOS/Linux forms redact; /home/anabel is not matched; char boundaries checked; a home of / leaks nothing; bridge.cjs sends exactly the text the dialog shows and the mail body has only typed fields. Minors (minors.tsv): names with - _ . in another folder, an account named like a segment, ASCII-only case | review report relayed in the session; crates/tc-app/tests/app/feedback.rs
