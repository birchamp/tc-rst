# HIST-1 lens: browser and parity (HEAD ca3ada86)

Summary as the lens subagent relayed it. tC4 reference: tc4-rc2 d25b7871, src/views/SyncResult.jsx. conflict-kinds.test.mjs passed 2/2. refusals 14/14. A throwaway Playwright pass covered the toast at 1280 and 620 wide, the Home link (Tab and Enter), and the Check and aligner History.

Blockers: none.

M1: grouped rows ("Titus 1:1 and 4 more") hid which records were replaced. tC4 lists each replaced alignment (#791). PLAN.md also wrongly said tC4 only counts. Fixed: one row per record, and the PLAN.md sentence is corrected.

Minors:
- The toggle had no aria-expanded. Fixed.
- The Restore buttons could not be told apart. Fixed with an aria-label that names the version.
- History text had no dir. Fixed with dir=auto.
- Labels are drawn in the scripture font. Logged.
- A check's History named only the verse. Fixed: it adds the check's title.
- The toast's rows were spans after the notes. Fixed: they are a list after "Changes from", and it scrolls.
- The Home link was indented when it wrapped. Fixed: it no longer wraps.
- "Settings can set it back." Fixed: "Project settings can set it back."
