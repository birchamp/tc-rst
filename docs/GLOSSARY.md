# Glossary

**Answers:** what a word used in this repository means. Where a document defines a word in
detail, it is named. **Wins over:** nothing.

## The product

| Word | Meaning |
|---|---|
| tC4 | translationCore 4, unfoldingWord's desktop app for Bible translation, written in JavaScript (Electron). The rebuild copies its look and behavior. Repository `unfoldingWord/translationCore4` |
| tC3 | The older translationCore 3. Its projects can be imported |
| The reference | The tC4 commit the screens that came from tC4 are compared with: `d25b7871` on `release/4.0` (4.0.0-rc.2) (ruling H7, `docs/DIFFERENCES.md` "The reference version"). tC4 4.0 is history: it no longer decides what the app does |
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
| Journey | A goal one person reaches, with its end state on screen and on disk: `docs/JOURNEYS.md`. J1 to J26 are tC4's, J27 and later the rebuild's. P1 needs each live one tested end to end |
| Commitment | One of V1 to V8 in `docs/VISION.md`: what the product promises translators. Journeys cite the ones they serve |
| Promise | One of P1 to P10 in `docs/PLAN.md` "The promises": true on `main` and shown by a check in CI. `docs/P-LEDGER.md` names the tests that show each |

## Storage (`docs/STORAGE.md`)

| Word | Meaning |
|---|---|
| Change | One immutable record of an edit (a verse text, a link, a decision), with its author and parents |
| Journal | The project's log of changes, on disk under `checking/` |
| Pending file | A file of new changes not yet sealed into a pack |
| Pack | A sealed, checksummed file of changes |
| Seal | Turning pending files into a pack |
| Fold | Computing the project's current state from all changes. Pure: it does no I/O |
| Conflict | Two versions of one verse or story frame text that nobody chose between yet. Always shown, never dropped. Every other kind of record takes the newest version and keeps the older in History (ruling H5) |
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
| Parity | Comparing the app's screens and components with tC4's. Done for 4.1.0 (`docs/rulings/2026-10-27-p7-review.md`), then retired (`docs/rulings/2026-10-29-tc4-is-history.md`). Its records stay in `parity/tc4-baseline`, `parity/ds-gallery` and `parity/view-gallery` |
| Gallery | Captured view states of tC4's screens (`parity/view-gallery`) |
| Residual | A known, explained difference from tC4 in the retired parity report |
| R-request | A request from the app team to the storage team (APP-DESIGN §9.2, R1 to R30) |
| Q-question | A question for the owner in APP-DESIGN §12 (Q-1 to Q-36) |

## Process

| Word | Meaning |
|---|---|
| Issue | A unit of work, a bug or a question, on GitHub at `unfoldingWord/translationCore4`. Work is tracked there since 2026-10-28 |
| Pull request | How every change lands on `main`, through the merge queue, with one approving review and the required checks (`AGENTS.md` "How work lands") |
| main | `main` of `unfoldingWord/translationCore4`. The old branch name `claude/focused-ride-5uzj7p` may still name it in a local clone |
| Card, unit | The retired work tracker's items (A1, C7a, S2, AQ1…), removed on 2026-10-28. Their last states are frozen in `scripts/tools/cards.json`, because documents still name them in `pending:` markers. A path written `tracker:<path>` is a file of that tracker, in the history of `main` at commit 9249a60d |
| Ruling | An owner's answer, one file in `docs/rulings/`; earlier ones are the `ruling` lines of `docs/rebuild.tsv` |
| Funnel | The method every change follows: code, then checks, then prose, then the owner (`docs/FUNNEL.md`) |
| Invariant (INV-…) | A rule that must always hold, named so tests and failures can cite it |
| REACH | The rule that only scenarios a translator can reach count (`AGENTS.md`) |
| The fix rule | Fix every blocker, and majors that harm a translator's data or what they see; the rest get one line in the pull request or an issue (`CONTRIBUTING.md` "Review findings") |
| Verification lenses | The three independent reviews a change that touches saved work gets, each by a reviewer who did not write it: **loss** (can any text, alignment, decision or comment be lost or shown wrong under crashes, reloads, a full disk and refused saves), **races and protocol** (duplicate, reordered, retried and interrupted requests, two tabs, a sync during a save), **browser and parity** (drive it in Chromium, compare with what tC4 does at the same moments, and check every difference is declared) |
| Op-count test | A test that counts the operations an action does, at Psalms scale and ten times larger |
| Model test | A property test that drives a state machine and a simple model side by side, with crashes and restarts |
| Mutation | A deliberate bug put into the code to prove a test catches it |
| Required checks | The CI jobs a pull request must pass in the merge queue (`AGENTS.md` "How work lands") |
| Windows VM | The owner's Windows 11 VM, for Windows builds and tests (`scripts/tools/winvm/`) |
| Reference laptop | The owner's Windows 10 laptop, used only to measure the speed budgets (`docs/PERFORMANCE.md`) |
