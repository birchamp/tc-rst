---
name: Bug (agent-ready)
about: A wrong result a translator can reach, written so a person or an AI agent can reproduce and fix it.
title: ""
labels: bug
---

<!--
Title: the wrong result in a translator's words ("A save during a sync is lost"), not a guess
at the cause ("Fix race in submit").
Search first: gh issue list --search "<words>". Only a scenario a translator can reach is a bug
here (AGENTS.md). Paste output verbatim. Delete these comments and any empty section.
-->

## What goes wrong

<!-- Journey (Jn) and what the translator sees, against what they should see. -->

## Steps to reproduce

<!-- From a person's action to the wrong result, through the app's own screens and API, or the
app or computer stopping, another program holding a file, a teammate's work arriving, a full
disk, or the network dropping. Say the starting project (fixture in `fixtures/` if one fits).
1.
2.
3. -->

- **Seen:**
- **Expected:**
- **Version:** <commit or release; OS>
- **Error text or log, verbatim:**

```text

```

## Where to look

<!-- Crates, files and functions involved. Label a suspected cause as a hypothesis, and say what
you saw that points to it. Name any working code that does the same job elsewhere
(CONTRIBUTING.md "Fix a bug", step 2). -->

## Done when

- [ ] A test through the public API named after the broken invariant fails today and passes with the fix
- [ ] It is shown to fail again with the fix reverted (docs/FUNNEL.md §2, step 4); the failure message is in the pull request
- [ ] Siblings searched for: the same pattern elsewhere is fixed here or filed as its own issue
- [ ] `scripts/gates.sh --touched` passes with `main` merged in

## Out of scope

<!-- Related problems the fix must not grow into. -->
