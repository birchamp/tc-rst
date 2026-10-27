---
id: USB-VERIFY
title: The USB export's check reads the stick, not the page cache
track: A
lane: storage
milestone: M4
state: todo
owner: none
branch: none
worktree: none
depends: [E2]
needs_owner: []
proves: []
no_loss: true
size: 1 d (basis: E2 step 3's verify, half a day, plus the §16.5 probe)
merged: none
updated: 2026-10-27
next: Claim this card after E2 merges, then do step 1.
blocked_by: none
---

# USB-VERIFY: The USB export's check reads the stick, not the page cache

## Goal

After "Save work for a teammate" says the file was saved and checked, the bytes on the stick
are the bytes that were checked. The verify reads the written file back from the device
(STORAGE §10.1 E5), or, where the platform cannot bypass the cache, asks the person to remove
and insert the stick again before the check (STORAGE §15 L85).

## Why it matters to a translator

A failing stick can take a write and give the cached bytes back: the app says "You can remove
the stick" and the teammate gets a damaged file. The project on this computer is untouched; what
is lost is the trip.

## Before you start

- `grep -n "fn verify" crates/tc-sync/src/exchange.rs` shows E5's verify reading the file with
  `std::fs` (through the page cache).
- `grep -n "L85" docs/STORAGE.md` shows the row this card owes a test:
  `usb_export_verify_reads_device`.
- STORAGE §16.5 lists the platform probe that shows which systems can drop a file from the cache
  (`posix_fadvise(DONTNEED)`, `O_DIRECT`, Windows `FILE_FLAG_NO_BUFFERING`).

## Funnel card

- **Invariants**:
  - INV-STICK-SENTENCE-AFTER-DEVICE-READ: the stick sentence is shown only after the file was
    read back from the device, or after the person re-inserted the stick and the read passed.
- **Wrong states**:
  - "Saved and checked" shown for a file the cache served (`usb_export_verify_reads_device`).
- **Code**:
  - Types:
  - One entry point:
  - Removed:
  - Left to checks, and why:
- **Checks**:
  - `usb_export_verify_reads_device` in `crates/tc-sync/tests/sync/exchange.rs`; kind: unit
    with a fault layer that serves stale bytes from a "cache" and other bytes from the "device";
    runs in: `cargo test -p tc-sync --test sync`.
  - Proof it can fail: with the verify reading through the cache, the test sees "saved and
    checked" for the stale bytes.
- **Prose**: STORAGE §10.1 E5 and §15 L85 name the test.
- **Human**: the re-insert question, if the platform needs it, goes to the owner with a screen.

## Steps

### Step 1: Probe the platforms

- Do: run STORAGE §16.5's cache probe on Linux, macOS and Windows; record which call drops the
  written file from the cache.
- Done when: `docs/evidence/usb-verify-<date>.md` holds the results.
- Record: log line `step 1`; update `next` and `updated`.

### Step 2: Read from the device, or ask

- Do: in `crates/tc-sync/src/exchange.rs`, drop the written file from the cache before E5's
  read where the probe says it works; elsewhere, return `ExchangeRefusal::AskReinsert` and let
  the dialog ask for the stick again before the check. Write `usb_export_verify_reads_device`;
  replace the pending marker in STORAGE §15 L85 and in `units/E2.md` with the test's name.
- Done when: the test fails with the cache bypass removed.
- Record: log line `step 2`; set `next` to the gates.

## Gates before merge

- `scripts/gates.sh --touched` passes.

## Log

- 2026-10-27 03:10 EDT | S2 session (Claude Code add49644) | created | none | split from E2 at its close: E2's card and STORAGE §15 L85 named `usb_export_verify_reads_device`, which E2 did not write (its verify reads through the page cache); the row now waits on this card | .audit/roadmap/units/E2.md
