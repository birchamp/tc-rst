# Glossary

Words this repository uses, in plain terms. Where a document defines a word in detail, it is
named.

## The product

| Word | Meaning |
|---|---|
| tC4 | translationCore 4, unfoldingWord's desktop app for Bible translation, written in JavaScript (Electron). The rebuild copies its look and behavior. Repository `unfoldingWord/translationCore4` |
| tC3 | The older translationCore 3. Its projects can be imported |
| The reference | The tC4 commit the rebuild is compared with: `d310bf3` today; it moves to tC4's `v4.0.0` tag when one exists (decision H7) |
| Translator | The person using the app. Every rule protects their work |
| Owner | The person who runs this rebuild and answers the questions only a person can |
| Door43 / DCS | unfoldingWord's Gitea server (`git.door43.org`): resources and team project repositories |
| Aquifer | A second source of translation helps (BibleAquifer). PLAN P6 |
| Scripture Burrito | The standard project format (a `metadata.json` plus ingredients). Every project the app writes is a valid Burrito (P5) |
| USFM | The text format of a Bible book (`\v 1 In the beginning…`) |
| OBS | Open Bible Stories: 50 stories of frames with pictures, a second project type |
| Gateway language | The language whose helps a translator reads (English, Spanish, Arabic…) |
| Original language | Hebrew (UHB) and Greek (UGNT) texts that alignments point to |
| tN, tW, tA, tQ | translationNotes, translationWords, translationAcademy, translationQuestions: the helps |
| ULT, UST | unfoldingWord's literal and simplified English texts, used as source texts |
| Pin | The exact version of a resource a project uses. Upgrades move pins (J12) |
| Check | One item of a checking tool (a note or a word) for one verse; a decision is the translator's mark on it |
| Alignment | The links between the translation's words and the original-language words of a verse |
| Translate, Check, Align, Understand | The four tools (modes) of a project |
| Journey | A tC4 user journey, numbered J1 to J26 in tC4's `docs/JOURNEYS.md`. P1 needs each live one tested end to end |

## Storage (`docs/STORAGE.md`)

| Word | Meaning |
|---|---|
| Change | One immutable record of an edit (a verse text, a link, a decision), with its author and parents |
| Journal | The project's log of changes, on disk under `checking/` |
| Pending file | A file of new changes not yet sealed into a pack |
| Pack | A sealed, checksummed file of changes |
| Seal | Turning pending files into a pack |
| Fold | Computing the project's current state from all changes. Pure: it does no I/O |
| Conflict | Two versions of one record that nobody chose between yet. Always shown, never dropped |
| Materialize | Writing the derived files (the book USFM, `metadata.json`) from the fold |
| Checkpoint | Seal, materialize and commit to the project's local git history |
| Placement ledger | S2's record of which bytes the app placed in each derived file, and its typed state machine (`ledger.rs`) that decides who wrote a file |
| Outside edit | A change another program made to a file the app wrote |
| Carried file | A file that travels with the project but is not derived (for example audio) |
| L-row | A row of STORAGE §15: one known way to lose work, and the test that proves it cannot happen |
| Format version | The version of the on-disk format. V freezes 1.0 |

## The app layer (`docs/APP-DESIGN.md`)

| Word | Meaning |
|---|---|
| `tc4 serve` | The Rust server program. It owns every project file and serves the UI |
| tc-api | The typed request and response types shared by the server and the UI |
| tc-app | The application services behind every screen |
| tc-ui | The Leptos UI, compiled to WebAssembly, with tC4's CSS and catalogs |
| Shell | The window program. In release it is Electronite; in tests any Chromium |
| Electronite | An Electron build whose Chromium shapes SIL Graphite fonts (P8) |
| Graphite | A font technology some scripts need; Electronite renders it |
| Save machine | The UI's single state machine for saving typed text (`Saves`) |
| Draft buffer | The browser-side copy of typed text that survives a crash |
| Port | A trait with a fake and a real implementation (`SyncPort`, `ImportPort`…), so tests and release run the same code |
| Parity | Comparing the Rust app's screens with tC4's (P7). Baseline: `parity/tc4-baseline` |
| Gallery | Captured view states of tC4's screens (`parity/view-gallery`) |
| Residual | A known, explained difference from tC4 in the parity report |
| R-request | A request from the app team to the storage team (APP-DESIGN §9.2, R1 to R30) |
| Q-question | A question for the owner in APP-DESIGN §12 (Q-1 to Q-36) |

## Process

| Word | Meaning |
|---|---|
| Unit | A planned piece of work from APP-DESIGN §10 or the STORAGE list (A1, C7a, S2, X…) |
| Work item, card | A unit or a new item of this roadmap, with its card in `units/` |
| Lane | A set of work items with one owner at a time (integration, storage, app, checks, domain, shell) |
| Track | A: the main line on the owner's PC. B: the second developer's package |
| Gate | The full list of checks a tree must pass before it merges (INT-3) |
| No-loss item (◆) | Touches a translator's saved work: three verification lenses and a model test |
| REACH | The rule that only scenarios a translator can reach count |
| The fix rule | Fix every blocker, and majors that harm a translator's data or what they see; the rest are minors |
| Minor | A finding logged in `.audit/minors.tsv`, not fixed now |
| Ruling | An owner's answer, one file in `rulings/` |
| Funnel card | The section of a card that walks the work through Code, Checks, Prose and Human (`FUNNEL.md`) |
| Invariant (INV-…) | A rule that must always hold, named so tests and failures can cite it |
| Op-count test | A test that counts operations an action does, at Psalms scale and ten times larger |
| Model test | A property test that drives a state machine and a simple model side by side with crashes and restarts |
| Mutation recipe | A deliberate bug put into the code to prove a test catches it |
| Enjoy | The other agent harness two of the sessions ran in on 2026-10-21; its logs are in `.enjoy-logs/` (not in git) |
| Windows VM | The owner's Windows 11 VM on this PC, for Windows builds and tests (`.audit/tools/winvm/`) |
| Reference laptop | The owner's Windows 10 laptop, used only to measure (`.audit/tasks/windows-track.md`) |
