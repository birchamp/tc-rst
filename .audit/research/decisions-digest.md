# tC4 decisions, risks, roadmap, packaging and evidence: digest for a Rust rebuild

Researched 2026-10-14 from `/home/user/unfoldingword/translationcore4/docs/` (read-only).
Sources read in full: `DECISIONS.md` (2766 lines, D1–D100), `RISKS.md`, `ROADMAP.md`,
`PACKAGING.md` (1092 lines), `MAP.md`, `LEGACY-IDS.md`, `plans/TEAM-SYNC-PLAN.md`,
`plans/LEGIBILITY.md` (first 150 lines), and the evidence files named in the brief. The
other evidence files were skimmed.

Citation forms:
- `DEC:NNN` is `docs/DECISIONS.md` line NNN, the heading line of the decision.
- `PKG:NNN` is `docs/PACKAGING.md`.
- `ev/<file>:NN` is `docs/evidence/<file>`.
- Platform facts are cited as tC4 cited them: pankosmia-web version and commit.

The decision log says "a recorded decision is not open for re-proposal" (`DEC:4`,
CONTRIBUTING hard rule 5). The rebuild team is not bound by that rule. This digest gives the
lessons behind each decision so the team can decide again with the evidence.

---

## 0. Context the decisions assume

The decisions only make sense against the current tC4 architecture:

- **Client.** React + Vite single-page app (`uw-tc4`), served at `/clients/uw-tc4`.
- **Server.** Pankosmia's `pankosmia-web`, a Rust Rocket server using libgit2. tC4 does not own
  it. tC4 pins a git rev of it: 0.18.15 at `a83725b` (D90, `DEC:2240`). The client reaches disk
  only through this server's HTTP API. The API has no append, no rename and no fsync
  (D50, `DEC:638`). No HTTP route writes `metadata.json` (D28 addendum, `DEC:265`).
- **Shell.** Electronite v37.1.0-graphite, unfoldingWord's Electron fork with SIL Graphite
  shaping (D20, `DEC:105`; `PKG:318-327`). The shell is the Pankosmia desktop-app-template at
  `4cb7576`, patched by `scripts/package-desktop.zsh`.
- **Format.** A project is a Scripture Burrito git repository. It holds USFM books, `checking/`
  sidecars (alignments, decisions, resource pins, settings) and an append-only per-actor journal
  under `ingredients/checking/journal/<actor>/segments/`. The format is specified in
  `BURRITO-SPEC.md` and proven by a conformance harness that must change in the same change set
  as the spec (§9).

Many decisions are workarounds for the Pankosmia HTTP boundary. A Rust rebuild that owns
storage and git in-process (git2 or gix, direct filesystem) can drop those workarounds. In the
relevance column below:

- **keep**: a product or data rule that does not depend on the stack. Adopt it.
- **reconsider**: the problem is real, but the answer was shaped by JS, Electron, Pankosmia, or
  by process or schedule. Decide again.
- **obsolete-with-Pankosmia**: the decision exists only because tC4 is a client of
  pankosmia-web, pankosmia-rcl or the desktop template. It goes away if the rebuild owns that
  layer. It stays if the rebuild keeps pankosmia-web as its server.

---

## 1. Every decision, D1–D100

D4 and D12 were never assigned (`DEC:10-12`). D28 has an addendum (`DEC:265`). D42 and D44 have
addenda (`DEC:587`, `DEC:544`).

| D | Line | What (one line) | Why (one line) | Rust rebuild | Reason |
|---|---|---|---|---|---|
| D1 | 15 | The design file is a style reference. Features come from the PRD. The check surface is design-native: tC3 RCL components are not embedded, but their read/write contract binds. | Design freedom without breaking tC3 data compatibility. | keep | The tC3 data contract is the compatibility surface. The UI is free. |
| D2 | 19 | Check semantics are triage (`valid/invalid/todo`) plus tC3 selections. Decisions persist in the full §5.2 shape, with an additive `status`. | Simplified records fail to round-trip into the tC3 contract (POC-2). | keep | Data model. |
| D3 | 22 | The prototype data layer is a FixtureStore over the sample burrito, behind the `BurritoStore` interface. An HTTP adapter comes later. | Build the UI before server integration. | reconsider | Keep the storage-trait idea. The fixture/HTTP split was Pankosmia-shaped. |
| D5 | 25 | Phase 1 scope is J1–J10 + J12. Team sync (J11) is Phase 2. | Ship single-user first. | reconsider | Scoping. D98 later redefined sync. |
| D6 | 27 | The gateway-language aligned Bible pane is out of Phase 1. | Scope. | reconsider | Scope. |
| D7 | 29 | Git author is user-controlled (pseudonyms allowed). No telemetry. Identity exposure is shown before the first push. | Translators in sensitive places. | keep | D85 found Pankosmia forces the author to the OS account name. A rebuild that owns git can honor D7. |
| D8 | 32 | The editor is byte-strict outside the edited verse: splice-based edits; `usfm-js` reads structure and never re-serializes the book. | Re-serialization corrupts and creates diff noise. | keep | Critical data-safety rule. |
| D9 | 35 | Save on verse blur plus idle debounce. Git commits only at checkpoints: session close, mode switch, before branch operations, before export or import. | No commit per keystroke; meaningful history. | keep | Re-check the commit cadence if the journal is the source of truth. |
| D10 | 38 | ULT/UST source panes are required in the first drafting increment. | Translators draft beside the source. | keep | Product. |
| D11 | 41 | tQ is out of Phase 1. | Scope. | obsolete | D64 put tQ in the book package. |
| D13 | 43 | The Phase 2 journal format is fully specified (BURRITO-SPEC §8), with a reference implementation and a conformance suite. Superseded in part by D47: the write side ships in 4.0.0. | Collaboration needs per-action history. | keep | Spec, reference implementation and suite move together. |
| D14 | 49 | Translator sections (`\ts\*`) are presentation only, derived at load from the pinned source. The target draft never contains `\ts\*`. | Keep the target text clean. | keep | Data rule. |
| D15 | 52 | Upstream accepted metadata fidelity in principle. The branch-merge endpoint request was withdrawn (local `file://` remotes suffice). Upstream asks are owner-routed and issue-sized. | Minimize upstream dependencies. | obsolete-with-Pankosmia | Upstream relationship policy. |
| D16 | 56 | Phase 2 topology: a full working projection, plus a persistent per-actor publication branch whose commits touch only the actor's journal. Named-branch integration in disposable scratch. Receive is validated rebuild-and-swap. | A git merge of full projections conflicts (`ev/pankosmia-sync-model-2026-07-08.md:71-104`). | reconsider | D98 sets it aside for round 1. Keep the principles: never merge into the working repo, zero-trust intake, rebuild-and-swap. |
| D17 | 61 | Pin sets: primary gateway language plus English fallback, each a coherent tn+tw+tA (+twl) suite. Resolution per (tool, book) by coverage, recorded in the decision file. Cross-language re-attach key: reference + original quote + occurrence, tiebroken by SupportReference / TWLink slug; ambiguous matches go to review. | Gateway languages have partial coverage (es-419_tn covered 14/66 books). | keep | Core resource model. |
| D18 | 82 | POC-2 learnings adopted. The endpoint surface works. Drafting ladder `\ts\*` → `\p` → verse. New books seed from the pinned source skeleton. Print-CSS is the publish lead. Forbidden: companion `*_tcchecks` repo, flat check records, index-pair alignments, blanket `no_bak`, whole-book writes. | Evidence from a working POC (`ev/tc4-poc2-learnings-2026-07-16.md`). | keep | Keep the ladder, the seeding and the forbidden list. The endpoint findings are Pankosmia-only. |
| D19 | 92 | A dev rig: scripted, seeded, pinned, isolated, with deterministic reset. Transport suite 10/10. Single-branch publication repos make `pull-repo` deterministic. PLATFORM-NOTES #20 and #21 found. | Reproducible integration testing against the real server. | keep | Keep the rig principle. The details are Pankosmia-only. |
| D20 | 105 | The desktop shell is Electronite, the Graphite-enabled Electron fork. Hard requirement. | Complex-script languages need SIL Graphite smart fonts; Chromium does not shape Graphite-only fonts. | keep (requirement); reconsider (mechanism) | See §2.7. Tauri's system webviews almost certainly lose Graphite (my inference). |
| D21 | 115 | Server round-trip suite 12/12. Custom work survives; regeneration wipes roles and relationships. A zip claim (withdrawn by D22). | Measure what the platform does. | obsolete-with-Pankosmia | The lesson stays: run a round-trip suite against any external store. |
| D22 | 123 | D21's zip claim withdrawn: the test used the wrong endpoint. `POST /burrito/zipped` imports the server's own export, byte-identical. | Correction from an upstream field report. | obsolete-with-Pankosmia | Lesson: name the endpoint under test; a field report outweighs one probe. |
| D23 | 133 | Owner rulings. The checking language pulls the whole suite; English fallback per (book, resource). Gateway change is explicit. The pin target is the sb-zip export; a pin is (repoPath, tag, expected SHA), verified at each import. Audio reserves a slot. Versification needs a spec. | Determinism and verification of resources. | keep | The pin-and-verify rule. sb-zip is DCS-specific. |
| D24 | 148 | Increment 1 includes source selection. Product repo created. (c) Refs are stored in the project's chosen versification; TSV and original-language refs map into it at derive time. | The storage-frame rule. | keep | (c) is a durable data rule. |
| D25 | 159 | Versification is a platform capability: six schemes, range-capable `mappedVerses`, `ingredients/vrs.json` per project, Proskomma mapping client-side, native passage sets. tC4 designs no mapper. | Reuse. Also: the upstream mirror was 15 commits stale; fetch before a platform claim. | reconsider | The server never maps (`ev/versification-format-and-frames-2026-08-24.md:162-192`). A Rust rebuild owns the mapper. |
| D26 | 170 | Passage sets are a minor UI option; whole books are the default. The data model must support scope ranges from the start: `currentScope` range arrays, progress denominator from scope, derive filters to scope, partial book files legal. No non-canonical books. | Almost all groups translate whole books, but the format must not foreclose passages. | keep | Data-model rule. |
| D27 | 185 | Rig pinned at `=0.18.3`, then git rev 0.18.5. Re-baseline transport and round-trip at every pin move. At 0.18.5 `relationships` survive regeneration; `x-` roles are wiped. Superseded by D90. | Upstream changes daily. | obsolete-with-Pankosmia | Lesson: pin, re-baseline, run a control on the old pin. |
| D28 | 217 | `x-` roles are non-durable by design. Stage rule S-2 ("paths are authoritative") is permanent. tC4 re-asserts roles client-side. Addendum (`DEC:265`): impossible over HTTP, no metadata write route. | Regeneration rebuilds ingredients from disk and cannot know `x-` roles. | obsolete-with-Pankosmia | A rebuild that writes `metadata.json` itself can keep roles. Keep "paths are authoritative" for robustness. |
| D29 | 233 | Adopt only the contexts of `pankosmia-rcl`, not its visual components. Exact pins. npm → pnpm planned. | Design consistency; MUI seams. | obsolete-with-Pankosmia | JS library choice. |
| D30 | 290 | The resolution unit is (tool, book). The fallback ladder has exactly two rungs. A project's pins bind every opener (retires tC3's reviewer-version drift). Missing pinned version online: fetch it. Offline: that (tool, book) is unavailable, a first-class state that blocks nothing else. Re-pin is explicit and warned. | Determinism across checker, reviewer and consultant. | keep | Core rules. D86/D88 amended points 4–5 (Download is a click). |
| D31 | 276 | Book writes: the splice engine plus a one-time creation seeder (only for a book with no text; verified against a usfm-js oracle). Passage-set UI deferred. Write-only pins for Increment 1. Push authorized to the tC4 repo. | Bound the write paths. | keep | The seeder rule. The rest is process. |
| D32 | 316 | TW and TWL import as separate burritos (sb-zip pins). No combining step and no combined artifact; the merge is an in-memory derive cache. | Byte traceability to the export; TWL and TW version independently. | keep | Reaffirmed by D96/D97 after D34 failed. |
| D33 | 334 | Mount Pankosmia's `Spa` + `AppWrapper` as an invisible shell for SSE context providers; restyle its toasts; fall back to a thin provider. | No headless provider upstream. | obsolete-with-Pankosmia | JS infrastructure. |
| D34 | 353 | One tW pin per language: the `<lang>_tw` sb-zip export carries both TWL TSVs and articles. `derive/` accepts both `rc://` and `./payload/` link forms. | Measured export shape. | obsolete | D96/D97: the export takes TWL from `en_twl` master at request time. Keep "accept both link forms". |
| D35 | 372 | Wordmap suggestions deferred. The disposable derive cache is not built (worst case 20.6 ms). The sidecar no-CAS/no-`.bak` gap (OQ #17) raised. Phrase alignment ships with suggestions. | Measure before building a cache. | keep | No derive cache needed (`ev/derive-perf-2026-08-03.md:9-17`). |
| D36 | 391 | The resource is the primary key. A decision that neither re-attach pass can place is marked `invalidated` + `invalid`, kept in full, not counted. Reconciled under compare-and-swap before the pins move. The dialog states "N carried over, M to check again". | A resource change means new work; progress must be honest; old decisions recoverable by re-pin. | keep | Data rule. |
| D37 | 413 | Store repo paths in DCS's own casing. Compare case-insensitively only (`samePath`). | No normalization on the way in or out. | keep | Small, correct. |
| D38 | 435 | Deferred work lives on the GitHub board as issues, not as sentences in documents. | Three of five "follow-ups" in a summary were undone after an increment, including Google CDN fonts that broke offline. | reconsider | Process. The lesson about offline fonts stays. |
| D39 | 458 | Single instance per machine; the in-process write lock covers same-machine concurrency. Cross-machine is the journal's job. Enforced by Electron's singleton lock in the packaged app. | Two layers of concurrency were conflated. | keep | See §2.1. |
| D40 | 487 | Every project maps TSV and original-language refs into its chosen frame at derive time. No eng-only design. Its own gated increment. | Correctness for non-eng projects. | keep | With D60's same-frame pass-through. |
| D41 | 510 | Open the installed fallback with a warning when the pinned primary is not installed; offer to fetch it. Record per-pin coverage in `resources.json`. | "Not local" and "does not cover this book" look the same. | keep | |
| D42 | 523 | Milestones and epics replace increment documents. Addendum: infrastructure increments need no journey. | Process. | reconsider | Process. |
| D43 | 530 | The repository's `docs/` is the single source of truth. | Process. | reconsider | Process. |
| D44 | 537 | The journal test plan folds into the spec as Appendix A. `LEGACY-IDS.md` frozen. (c) Superseded decided text is annotated, never rewritten. | Process. | reconsider | The annotation rule is a good habit. |
| D45 | 555 | No knowledge-graph store for decisions. | Documents plus disciplined IDs suffice. | reconsider | Process. |
| D46 | 563 | SemVer pre-releases of 4.0.0 (`alpha.N` → `beta.N` → `rc.N`); never 3.9.x. | Mark the rewrite boundary. | reconsider | A Rust rebuild needs its own version story. |
| D47 | 571 | (a) After 4.0.0, no at-rest format change without a `schemaVersion` bump, a migration and old-data fixtures. (b) Before 4.0.0, rank by "does postponement accrue bad data?". (c) The journal write side ships in 4.0.0: every project has full history from day one, verified in CI by folding. (d) Verse moves post-4.0 (superseded by D70). | Bad data compounds; history cannot be added later. | keep | The most transferable process rule. |
| D48 | 594 | §8 conditionally ratified. The journal architecture is settled: append-only actor journals, deterministic fold, explicit forks, derived projections, isolated publication, zero-trust integration, rebuild-and-swap. Seven contract corrections. `textMd5`/`skeletonMd5` removed. | Three independent reviews. | keep | Core architecture. |
| D49 | 628 | The packaged app uses a product-isolated project store. Production starts empty. A debug build seeds a separate, marked store. The smoke test fails a build that resolves the shared store. | The platform default `$HOME/pankosmia_repos` is shared by every Pankosmia app. | keep | The store-path details are Pankosmia-only. |
| D50 | 638 | Store-to-journal boundary. Every mutation goes behind `BurritoStore` with a per-project queue (observe → diff → durable intent → publish → regenerate). Each mutation is an immutable sealed segment (one JSON container, body sha256, 4 MiB limit; parse/checksum validity is the commit marker). A durable outbox in IndexedDB. Four-way recovery. Runtime fold. Universal seeding. Actor ownership from an installation secret. Crash-atomic; power-loss detected, not guaranteed. | The HTTP boundary has no append, rename or fsync. | keep (principles); reconsider (mechanism) | With direct filesystem access, use write-temp + fsync + rename + dir fsync, and drop the IndexedDB outbox. |
| D51 | 660 | §8 becomes normative in reviewable steps, not one change set. Every normative statement needs a check that fails when it is violated. Generators must reach the states they claim. Fix the class, not the instance. Every step leaves `main` self-consistent. | PR #75: 31 commits, 11 review rounds, 306 checks green, five blocking defects. | keep | Process lesson about green suites. |
| D52 | 724 | The custom journal is retained. Automerge, Yjs and Loro were tested; none replaces it. | Automerge passes safety only in a constrained role, costs a WASM dependency and a key lifecycle, is slow on long histories, and deletes no code. | keep | See §6. Re-run the benchmark if the rebuild considers native `automerge` (Rust). |
| D53 | 801 | Work starts with no account and no shared root. Joining is not merging; merging two projects is an explicit human act. Actor identity is per project. `base: null` means "no prior state I know of": identical payloads converge, different ones fork. A merge shows conflicts and never blocks. A resolution is an event. | The product exists to remove the account barrier. | keep | Core collaboration semantics. |
| D54 | 867 | Prefix-related dotted paths (`ui.pane` vs `ui.pane.width`) resolve by later `ts`; the loser is retained as `prefix-collision`. | No silent loss. | keep | Or forbid prefix paths in the schema. |
| D55 | 879 | §8 write format and fold invariants are normative. Sync operations stay [PROPOSED]. Every normative rule has an `[R-…]` id claimed by a live check; CI fails on an uncovered id. App rules carry no R-id. | "If users lose a book of scripture then the app is dead." | keep | The R-id coverage gate is cheap and strong. |
| D56 | 925 | Sidecar record order is byte form: the seed round-trip compares records as a set. The sample burrito must be seedable. Unknown top-level sidecar fields refuse the seed. | Real tC3 exports store records in authoring order. | keep | |
| D57 | 949 | A suite with no recorded version is readable but not pinnable. | The journal schema refused empty versions. | obsolete | Dissolved by D58. |
| D58 | 966 | The commit sha is the pin's identity (40 hex). The version tag is an optional display label. Fetch still speaks tags. | Tags are not enforced upstream; sample lexicon tags did not exist. | keep | |
| D59 | 992 | A version label resolves to a sha at the import boundary by one DCS tag lookup. Every version-identity fallback is deleted. The relabel guard compares repoPath + sha. | tC3 projects record only owner/repo/tag. | keep | |
| D60 | 1023 | Same-frame references pass through unvalidated (`front`, comma lists, letter verses). Only a cross-frame conversion rejects what it cannot compute. Identity safety stays in the journal grammar. | The strict rule dropped 112 en_tn rows, 110 of them PSA `N:front`. | keep | |
| D61 | 1042 | A pin's recorded coverage may be widened from a sha-exact local read: add-only, never shrunk. | The first capture can read a partial copy. | keep | |
| D62 | 1056 | A whole-collection resource records `["BIBLE"]`; never mixed with book codes. tA records nothing. | The platform's own marker; without it a tW install fell back to English. | keep | Or define an equivalent marker. |
| D63 | 1073 | Understand / Translate / Check workflow on a new design system. Publish moves into Check's Community Checking tool. | Owner design work. | keep | Product UX. |
| D64 | 1078 | tQ and the simplified text join the book package as OPTIONAL language-set slots. | English ships UST and tQ. | keep | Additive. |
| D65 | 1092 | User comments save through their own SaveScheduler instance: per-key latest-value buffer, 2 s debounce, flush-and-go navigation, one `note.add` per flush. | 22 adversarial review rounds found ~21 defects in hand-rolled note-save machinery. | keep | One write discipline for all text input. |
| D66 | 1113 | Open Bible Stories is a first-class project kind, `gloss/textStories`, distinct from the Bible model. | Stories and frames are not books and verses. | keep | |
| D67 | 1139 | Team sync is one reference engine (send/integrate/receive) over a repository port with memory, git and HTTP adapters, after a legibility increment. Receive rebuilds from four inputs (team main, own publication, own unsent segments, outbox). Swap recovery is an intent record. One integrator per offline team. A new device is a new actor. Anyone may integrate. | An audit found receive dropped the actor's unsent work. | reconsider | D98 sets most of it aside for round 1. The port/adapter pattern and the four-input receive stay valuable. |
| D68 | 1219 | The `generation` stamp is mandatory only on the three ops whose first write may be rootless (`align.verse.set`, `check.decision.set`, `note.add`). A verse edit takes its generation from its `base` chain. | The spec sentence was over-broad. | keep | Detail of the fold. |
| D69 | 1250 | A journey is defined by the on-disk end state, never UI state. Journal checks renamed `JC-n`. `MAP.md` lists every doc and id prefix. | Testable, stable journeys. | keep | "On-disk end state" is the best test principle in the repo. |
| D70 | 1276 | Verse spans ship in 4.0.0 as one `text.structure.apply` action with the conservative disposition (alignments and decisions invalidated and retained, never re-keyed). Renumbering later. The gesture drags verse numbers, never text. | The format already supported it. | keep | |
| D71 | 1318 | The lexicons are the `uW`-org burritos, pinned by commit sha, fetched as Gitea commit archives verified against the zip's archive comment. Hardcoded until the December 2026 Greek NT brings its own lexicon. | No tag and no sb-zip export exists for them. | keep (pattern); reconsider (target) | Rework in early 2027. |
| D72 | 1348 | Increment 6. Suggestions learn only from the project's own confirmed alignments, in a Web Worker; an unconfirmed suggestion is never written; Mark valid is refused while one stands. Check comment = §5.2 `comments`; bookmark = `reminders`. Resource upgrade per language set, pins move only after every resource is installed and sha-verified. | Owner rulings. | keep (rules); reconsider (engine) | |
| D73 | 1406 | A verse's alignment is done when every word is placed, or when the translator marks it valid. Optional additive `done` flag; any edit recomputes it. | "Bank empty" was not "reviewed"; tC3 had a checkbox. | keep | |
| D74 | 1436 | Increment 7 is OBS. The pankosmia `text_stories` template byte for byte. The image line splits frames; one paragraph per frame; locator `story:frame`; frame 0 = title. Journal vocabulary v2. Picture pack (49.1 MB) in each installer. | Interop with Pankosmia's OBS form. | keep | Format decisions. |
| D75 | 1520 | OBS sets use OBS-specific completeness (obs, obs-tn, obs-twl, tw, tA). An optional `obs-images` pin; the bundled pack separate. `obs-tq` optional. | | keep | |
| D76 | 1542 | OBS Understand shows the source only. | Parity with Bible Understand. | keep | |
| D77 | 1546 | The vitest passed count is not written in documents. | 47 of 325 commits were count refreshes. | reconsider | Process. |
| D78 | 1562 | The vitest skip count is not written either. Only conformance counts are marked. | The skip count moved 4 times in 17 days. | reconsider | Process. |
| D79 | 1589 | Increment 8 publish/import; 8.5 first share. PDF by the print-styled route in the same Chromium (not Pankosmia's headless-Firefox publisher). Export lands as a browser download. Four outputs. A `relationships` mirror on export. Import creates a new project only. First share pushes the working `main` (a transport action, not sync). Credentials in memory for the session. | Offline pilot forbids a run-time browser download; scope to the date. | keep (PDF same engine, offline); obsolete-with-Pankosmia (import via remake route) | |
| D80 | 1700 | Export says "Saved" only after a completed download (a `will-download` bridge). A Scripture Burrito import keeps its journal. A tC3 import keeps the current state only. Page setup stays in memory. Import validation is tC4's own. | No Pankosmia shell reports download results. | keep | The language-subtag workaround was superseded by D90. |
| D81 | 1774 | Playwright journeys are the primary test method. Keep a unit test only when it catches a defect the journeys miss. List the failure modes first for isolated tests. | 1340 → 1034 unit tests; many could not fail. | reconsider | CI does not run the journeys, so journey-only defects surface late. |
| D82 | 1796 | A tC3 import stores no pin without its sha. The review page looks up each version on DCS. Offline: go online or use installed versions. Different licenses: the user chooses. Different original-language versions: pin the newest on DCS. | The format has no unresolved-pin form. | keep | |
| D83 | 1829 | An import never chooses a license. | "We don't get to decide the licenses for people." | keep | |
| D84 | 1847 | Share lives on the Home card. To the user's account or an org with `can_create_repository`; recommend the org with the most repos in the project's language. "Stay signed in" keeps the token in the OS keychain (`safeStorage`). | Owner mockup. | keep (UX); reconsider (keychain mechanism) | Rust: the `keyring` crate. |
| D85 | 1902 | Store only the Door43 token, in the keychain, only with "Stay signed in". No name, email or login. The notice says the computer's account name signs each change. | The platform sets the author to `whoami` and no route changes it. | keep (minimal storage); obsolete-with-Pankosmia (author) | A rebuild that owns git can set the signature (D7). |
| D86 | 1940 | A persistent "Internet"/"Local" status in the top bar. | Users read "offline" as a fact about the computer. | obsolete | Replaced by D88, then D95. |
| D87 | 2062 | A project with no remembered place opens in Understand. | Read before translating. | keep | |
| D88 | 2082 | An account menu replaces the status. A per-task "Use the internet?" dialog. Consent is separate from the platform net gate. | One more state to manage; ask at the moment data leaves. | obsolete | Points 2–4 replaced by D95. |
| D89 | 2166 | Increment 9 ship rules. rc.2 on 2026-10-12, v4.0.0 on 2026-10-16 promoting rc.2. A red gate moves the date; a red feature leaves. P2/P3 cut on 2026-10-09. macOS unsigned. Only the `uw-tc4` client. Help pages pinned offline. "Report a problem" via SendGrid with a build-time token. Rollback plan and usfm-js upgrade to 4.0.1. | Six working days left. | reconsider | Release management. Keep offline help and "a gate moves the date". |
| D90 | 2240 | Pin pankosmia-web 0.18.15 (`a83725b`). Regional tags (`es-419`) kept whole. A Scripture Burrito import is stored as exported except the commit fields of `metadata.json`. The OBS scope is compared by keys. | Upstream changes; from 0.18.10 each commit rewrites `metadata.json`. | obsolete-with-Pankosmia | Lesson: a server that rewrites metadata on commit broke byte identity. |
| D91 | 2299 | The journal owns project settings; `checking/settings.json` is a generated mirror. `checkingLanguage` is obsolete. | It was never read; the pins decide. | keep | |
| D92 | 2327 | A Scripture Burrito import rewrites `identification.name` and `abbreviation` to the typed name and folder. | The typed name was lost (#499). | keep | |
| D93 | 2352 | Suggestions grow with every saved verse: append to wordMAP memory (0.14 ms), retrain the booster in the background on a budget, in its own worker. | Built literally, D72 retrained for minutes per saved verse. | keep | |
| D94 | 2416 | rc.1 QA defects stay in scope until rc.2; acceptance proven by journeys. | Two rules conflicted. | reconsider | Process. |
| D95 | 2440 | The internet is one on/off state for the app session. Each launch starts off; nothing is stored. A step that needs it asks in place; "Turn on internet" lasts the session. Every request outside the local server is refused before sending while off. | Per-task consent holds leaked across seven review rounds. | keep | See §2.10. |
| D96 | 2537 | The English suite pins `en_twl` (TWL) and `en_tw` (articles) separately, each at its own release and sha. | The `en_tw` sb-zip is built per request with TWL from `en_twl` master: unreproducible, and it shipped 7 unreleased books. | keep | |
| D97 | 2574 | The one-repo tW form is illegal: the TWL pin must name a `_twl` repo. A gateway with no `_twl` uses the bundled `en_twl` with its own `<lang>_tw` (the only mixed-language set). tC3 import picks the `_twl` release closest in date. | Same root cause as D96. | keep | |
| D98 | 2635 | Team sync round 1 (4.1.0): one public Door43 repo, `main`, one "Sync with Door43" button, no publication branches, no integrator, no legibility gate. Guarantees G1–G8. Usernames in the journal (`actor.name.set`, journal v3). Only verse-text conflicts reach a person; checks and alignments: newest wins. | Each change is already one sealed file per actor; the fold combines journals; no git merge of project files is needed. | keep | The most relevant sync design. See §2.4. |
| D99 | 2694 | Windows v4.0.0 ships signed (Azure Artifact Signing through GitHub OIDC). Signing is a release gate. Every unsigned `.exe`, `.dll`, `.node` is signed. rc.2 unsigned, signed rc.3, v4.0.0 promotes rc.3. | An unsigned build can start with no window and no warning. | keep | |
| D100 | 2738 | `ar_gt` leaves the gateway registry; BSOJ is the only Arabic package. | `ar_gt/ar_tw` v3: 420 of 422 articles are Persian under the tag `ar`. | keep | Validate gateway content, not only metadata. |

---

## 2. Decisions by subject

### 2.1 Data safety and write discipline

**The standard.** D55 quotes the owner: "This has to be right. It's too valuable. … if users lose
a book of scripture then the app is dead" (`DEC:881-883`). Everything below serves that bar.

**Text edits are byte-strict (D8, D31, D18).**
- The editor splices one verse. `usfm-js` reads structure and never re-serializes the book
  (`DEC:32-33`). The POC wrote the whole book on each save; D18 forbids that (`DEC:88-89`).
- Only two code paths write a book: the splice engine and a one-time creation seeder. The seeder
  runs only on a book with no text, and is verified against an independent usfm-js oracle over the
  real corpus (`DEC:277-280`).
- `.bak` undo stays for USFM writes; `no_bak` only for high-frequency sidecar writes
  (`ev/tc4-poc2-learnings-2026-07-16.md:61-62`).
- D35(b) raised a gap: sidecar writes had no compare-and-swap and no `.bak`, unlike book writes
  (OQ #17, `DEC:381-383`). D36 and D61 later require compare-and-swap for decision and
  `resources.json` writes (`DEC:401-404`, `DEC:1053-1054`).

**Save cadence and commits (D9, D65, D81).**
- Save on blur plus idle debounce. Git commits only at checkpoints (`DEC:35-36`). The resume
  record shows "Commits 1 throughout: a save is not a checkpoint"
  (`ev/resume-restart-2026-09-05.md:80`).
- D65 (`DEC:1092`): one SaveScheduler discipline for all text input. The buffer is a per-key
  latest-value register, so a retry replays only the newest text, and the buffer is the draft
  store across unmounts. The failure slot is per scheduler instance, so a failing comment does not
  park verse autosave. Navigation flushes and goes; it refuses only while a write failure stands.
  This replaced hand-rolled machinery that 22 adversarial review rounds could not stabilize.

**Concurrency has two layers (D39, D49).**
- Same machine: a single instance per machine plus an in-process path lock
  (`httpStore.withPathLock`) covers all overlapping writes. A cross-tab `navigator.locks` guard was
  reverted as dead code (`DEC:462-466`).
- Different machines: not a lock problem. Each machine has its own clone; the journal resolves it
  (`DEC:467-471`).
- The single-instance guarantee is enforced, not assumed. The packaged `tc4-main.js` takes
  Electron's singleton lock before the template startup loads; the smoke test fails a build where
  a second launch survives or spawns a second server (`DEC:478-484`, `PKG:110-140`).
  Measured: the template launcher had no lock; a second launch scanned to the next free port and
  started a second server over the same store (`PKG:116-119`). tC3's lock never quit the second
  copy; macOS Finder semantics hid it (`PKG:120-123`).
- D49: the store is product-isolated (`$HOME/pankosmia/tc4-projects`) so no other Pankosmia app
  can write tC4 projects. This removes a cross-application concurrency class (`DEC:628-636`).

**The durability posture (D50).** Process-crash atomicity is guaranteed. Power-loss durability is
detected but not guaranteed: "losing the final seconds-to-minutes of typing in a power outage is
accepted"; the platform fsync primitive is an open upstream question (`DEC:651-654`). The
mechanism exists only because the HTTP boundary has "neither append nor rename/fsync"
(`DEC:645-646`). A Rust rebuild with direct filesystem access can close this: write to a temp
file, fsync, rename, fsync the directory.

**Honest UI about saves.**
- D80(1): an export says "Saved <file>" only after Electron reports a completed download
  (`DEC:1749-1753`). Pankosmia clients show "saved" when the download starts (`DEC:1727-1730`).
- D53(e): a forked verse projects its highest-`ts` head, which may not be the reader's own text.
  The UI must show the fork, "otherwise a translator sees their own work apparently replaced with
  no explanation" (`DEC:862-865`).

**Format change contract (D47).** After 4.0.0, no at-rest format change lands without a
`schemaVersion` bump, a written migration and harness fixtures that prove old data opens under the
new reader. Additive-optional fields are the only exception (`DEC:572-575`). D73 and D91 use the
additive exception (`DEC:1427-1430`, `DEC:2313-2321`).

**Risk 10 (`RISKS.md:18`).** v4.0.0 accepts journal envelope `v` 1 and 2 only. An older build that
meets a `v: 3` journal (D98's name action) refuses it as "A saved change in this project is
damaged", not "made by a newer version". A newer-version refusal must ship before the first build
that writes `v: 3`.

### 2.2 Storage and the on-disk format

- **One repo per project, everything inside it.** POC-2's companion `*_tcchecks` repo is
  forbidden; checking data lives in `checking/` sidecars inside the project repo, full §5.2 records
  (`ev/tc4-poc2-learnings-2026-07-16.md:53-60`; D18 `DEC:87-89`).
- **Paths are authoritative, metadata is not (D21, D28, S-2).** Pankosmia's regeneration rebuilds
  the `ingredients` map from disk and wipes `x-` roles; at 0.18.5 `relationships` survive
  (`DEC:209-211`). So tC4 never relies on `metadata.json` roles; `checking/resources.json` is the
  authoritative pin record (D79(6), `DEC:1631-1636`). From 0.18.10, each `add-and-commit`
  rewrites `metadata.json` (`dateCreated`, revision, timestamp, roles) and sorts keys
  (`ev/rig-rebaseline-0.18.15-2026-10-03.md:50-75`; PLATFORM-NOTES #48). This broke byte-identity
  of imports (D90(6), `DEC:2273-2281`) and the OBS scope order (D90(7)).
- **Sidecar record order is byte form (D56).** Compare records as sets in the fold's canonical
  order; unknown top-level fields refuse (`DEC:936-947`).
- **The resource is the primary key (D36).** See §2.5.
- **Settings are journal-owned (D91).** `settings.set` events (last writer wins per path) fold to
  values; `checking/settings.json` is regenerated whole and never read as authority
  (`DEC:2301-2305`).
- **Scope (D26).** `currentScope` permits SB's range-array form (`{"TIT": ["1:1-2:5"]}`); `[]`
  (whole book) is the default. Progress denominators come from the scope, and a book file may hold
  only some verses (`DEC:174-179`).
- **OBS (D66, D74, D75).** The pankosmia `text_stories` template byte for byte:
  `ingredients/content/01.md`–`50.md` plus front/back files. One title line; per frame one image
  line and exactly one paragraph; a reference line. The image line splits frames. Frame writes are
  byte-strict outside the frame. No `vrs.json`, no alignment sidecar (`DEC:1456-1468`).
- **Disk size at scale.** The checked-whole-Bible journal is 247.6 MiB in 278,041 files
  (avg 934 B), about 1.1 GiB allocated on APFS because each small file takes a 4 KiB block
  (`ev/bench-fold-2026-08-25.md:173-179`). gzip gives 2.33x per file and 17.62x as one stream
  (`:166-171`). One file per action has a real filesystem cost. My suggestion, not a tC4
  decision: a Rust rebuild should consider packing sealed segments into larger immutable files
  (the per-action seal can stay).

### 2.3 The journal

The journal is the central data-safety mechanism. Its decision trail: D13 (spec), D16
(topology), D47(c) (write side ships in 4.0.0), D48 (architecture approved), D50 (store
boundary), D51 (staged ratification), D52 (no CRDT library), D53 (identity and base semantics),
D54 (prefix rule), D55 (normative flip and coverage gate), D56, D68, D91, D98.

**What a journal is (D48, D50).**
- Append-only actor journals. Each mutation is an immutable sealed action segment: one JSON
  container, sha256 of the body string, 4 MiB limit. Parse plus checksum validity is the commit
  marker. No appends and no rewrites of accepted files (`DEC:642-646`).
- Segment file name example: `fixture-large\segments\2026-09-01T00_00_00.001Z,0000,fixture-large.action.json`
  (`ev/open-time-windows-2026-09-28.md:63`): an HLC-style timestamp, a counter, the actor.
- A durable outbox in IndexedDB holds exact bytes for retries; it is installation-local and never
  committed. Recovery classifies four ways: publish-from-outbox, regenerate, converged,
  out-of-band (`DEC:646-649`).
- Every other project file (USFM, sidecars, settings) is a projection of the fold, regenerated at
  checkpoints (R-8.7.1, cited in D98 `DEC:2640-2643`).
- Actor ownership binds to an installation secret, not a hardware fingerprint (`DEC:650-651`).
  D53(c): one installation has a separate actor id per project, derived from the secret
  (`DEC:824-832`); the id also derives from the repository path (D67(3), `DEC:1177-1178`).

**What the fold guarantees (D48, D53, D54, D55).**
- Deterministic fold, conservation (nothing silently lost) and exclusivity (`DEC:885-887`).
- Different actors editing the same key from the same `base` produce a fork that keeps both
  versions. The same actor's edits are linear (`DEC:827-830`).
- `base: null` means "no prior state I know of". Identical rootless payloads converge as one
  fact; different ones fork (`DEC:834-842`). Deterministic seeding plus convergence makes the
  format self-healing: a project that arrives without its journal is re-seeded on open and
  converges (`DEC:844-847`).
- A resolution is an event that supersedes both heads; arrival order changes nothing; two
  different independent resolutions fork again (`DEC:855-860`).
- Prefix-related paths: the later `ts` wins; the earlier is retained as `prefix-collision`
  (`DEC:867-874`).
- Generations: `book.add` starts a book generation; the `generation` stamp is mandatory only on
  `align.verse.set`, `check.decision.set`, `note.add` (D68, `DEC:1219-1246`).
- Structural changes (verse spans) are one atomic `text.structure.apply` action carrying the old
  → new key mapping; Increment 5 uses only the conservative `invalidate-retain` disposition
  (D48(3), D70, `DEC:610-612`, `DEC:1285-1289`).
- Vocabulary versions: v1 Bible ops; v2 adds `text.frame.set`, `text.story.ref.set` for OBS
  (D74(7), `DEC:1472-1475`); v3 adds `actor.name.set` (D98(5), `DEC:2653-2658`).

**How it was ratified, and what went wrong (D48, D51, D55).**
- D48 demanded one change set with all conditions. PR #75 grew to 31 commits across 21 files and
  11 review rounds. A four-lens adversarial review found five blocking defects, two of them data
  loss or availability failures, while the branch suite reported 306 checks passing. The suite
  grew by accretion (59 → 306) because each round added a test for the instance, not the class
  (`DEC:666-675`).
- D51's rules from that failure: every normative statement needs a check that fails when it is
  violated; a property generator must be able to reach the states its property describes (a
  generator that re-folds after every event cannot build a fork); fix the class at one shared
  definition; every intermediate merge stays self-consistent (`DEC:687-709`).
- D55's gate: each normative rule carries an `[R-…]` id, and `conformance/normative/check.mjs`
  fails on any id that no live check names (`DEC:898-906`). The known data-loss defects were fixed
  red-first. Residuals: per-statement mutation proof is not done for every rule; three-device
  join generation is open (`DEC:908-916`).
- Coverage at publication: format conformance 48/48, journal 350/350, normative rules 91/91 at
  alpha.7 (`ev/offline-run-2026-09-22.md:110-114`).

**Performance of the journal.** See §5. In short: the fold of a checked whole Bible (278,041
events) took 9.3 s, cut to about 5.6 s; open over HTTP read every segment twice and was projected
at three to four minutes for a whole Bible; a 40,000-segment project opened in 81–90 s. The fold
moved into a Web Worker. The per-request HTTP cost was replaced by one zip read.

**What a Rust rebuild should take.** The fold rules and the format are implementation-neutral.
The reference fold is JavaScript (`journal/fold.mjs`); a Rust port needs the conformance suite
as its oracle, run as a cross-implementation check. The HTTP-driven mechanics (IndexedDB outbox,
segment-per-request reads, double scans) are not needed with direct file access.

### 2.4 Sync, merge and collaboration

**What Pankosmia offers, read from its source (`ev/pankosmia-sync-model-2026-07-08.md`).**
Three repo locations (local, `downloaded` mirror, disposable `_updates_` scratch). The merge runs
only in scratch; on conflict the scratch is deleted and the user's repo stays pristine; on
success the user's repo fast-forwards (`:22-34`). Verdict: it avoids corruption but does not
resolve. Two translators who edit the same or adjacent lines hit "merge conflicts" with no in-app
way forward. There is no resolver, no lock and no rebase anywhere (`:45-59`). Upstream called
this "hasn't ever worked in the last decade" (`:3-6`). tC4 adopted the scratch principle and
rejected the line merge.

**Why a git merge of the project fails.** A checkpoint commits regenerated USFM, metadata,
sidecars and settings, and the server's `add-and-commit` stages `.`. A real-git A1 → B1 → A2
reproduction conflicts on the full working history (`ev/pankosmia-sync-model-2026-07-08.md:71-79`).
The transport rig also found: `pull-repo` picks `FETCH_HEAD` by ref order, so multi-branch
repos are unsafe (`ev/transport-rig-2026-07-18.md`, T2); after a normal merge the worktree is not
trustworthy and a following commit can delete merge-added files (PLATFORM-NOTES #21).

**The long-term design (D16, D67, TEAM-SYNC-PLAN).** Per-actor publication branches whose commits
touch only the actor's journal; integration in disposable scratch; zero-trust intake (refuse
shared-file changes, foreign-actor segments, rewrites of accepted bytes); receive by validated
rebuild-and-swap. D67 adds: one engine over a repository port with memory, git and HTTP adapters;
receive from four inputs, because an audit found the actor's unsent segments and outbox were not
inputs (`DEC:1141-1151`); swap recovery by an intent record, because Pankosmia's move is copy then
delete and a crash leaves two copies (`DEC:1186-1192`). Invariants I1–I8 and the work items
(X2, S1–S9) are in `plans/TEAM-SYNC-PLAN.md:141-192`, `:249-468`.

**Round 1 replaced most of it (D98, 2026-10-13).** The owner chose a simpler design for 4.1.0
(`DEC:2635-2692`):
- One public Door43 repository, its `main` branch, one "Sync with Door43" button.
- One press: get Door43 `main`, add own journal files, rebuild project files from the fold, push,
  swap in at the same path.
- Why it works: each change is one sealed file in the actor's own folder, two translators never
  write the same file, and every other file is rebuilt from the journals. "So a sync needs no git
  merge of project files" (`DEC:2637-2643`).
- Only verse text reaches a person, in a translationStudio-style verse-by-verse review. Check
  results and alignments: the newest wins (`DEC:2666-2667`).
- No publication branches, no integrator role, no legibility gate (`DEC:2660-2665`).
- Usernames go in the journal (`actor.name.set`, journal v3), amending D85.
- Guarantees with test ids (`DEC:2673-2682`): G1 own work never lost (byte for byte, or the sync
  changes nothing); G2 every incoming journal file passes the validator, one failure stops the
  sync; G3 derived files are rebuilt, never trusted; `metadata.json` is never written by a sync;
  G4 never force-push, a non-fast-forward restarts the sync; G5 same path, same actor id; G6 same
  journals give the same project; G7 only verse text reaches a person; G8 a crash during the swap
  never leaves two projects or loses own work.

**Transport contract measured (`ev/sync-transport-spike-2026-10-13.md:43-113`).** Full clone
(no `branch`, which would make it shallow) to a fixed path, move by copy+delete, push with
credentials in the body. Non-fast-forward is HTTP 500 with `NotFastForward`. A missing username
or token panics the server inside the credential callback (`push.rs:83-86`); the client must
check first. A rejected Door43 token gives a misleading libgit2 text ("authentication that we do
not support"); match on `code=Auth (-16)`. Refusals after the pack (protected branch) are not
reported by the push route. A clone of an empty repository yields `HEAD` at a missing
`refs/heads/master`, and a push from it panics (`:600-604`).

**Before sync (4.0.0): the first share (D79(12), D84, D85).** Create a repo under the user's
account or a permitted org and push the working `main`. Never a force push. Later shares push
`main` again. A second device that pushes first makes the next share refuse
`share.non-fast-forward` with "team sync is coming and local work is safe" (Ledger #6).
Risk 5: a share-born `main` is not yet proven equivalent to an integrate-born team main; scenario
X8 must run before a sync feature ships (`RISKS.md:13`, `plans/TEAM-SYNC-PLAN.md:321-343`).

**Merge semantics independent of transport (D53).** Start with no account and no shared root.
Joining is not merging. Merging two projects is an explicit human act that the system never
infers or proposes from a matching language, book or name (`DEC:817-822`). A merge completes at
once, shows conflicting verses, offers take-all-mine / take-all-theirs / resolve-while-working,
and never presents a wall of modal decisions (`DEC:849-853`).

**For a Rust rebuild.** D98's design fits a Rust core well: git2/gix for clone and push, the fold
in-process, an atomic directory swap with an intent record (and a real `rename`, not a copy plus
delete). The G1–G8 table is a ready test plan.

### 2.5 Resource pinning

The decision trail: D17, D23(b), D30, D32, D34, D36, D37, D41, D57, D58, D59, D61, D62, D64, D71,
D72(5), D75, D82, D96, D97, D100. It is the area with the most corrections.

**The model.**
- `checking/resources.json` (§5.3, schemaVersion 2) pins two language sets, `primary` and
  `fallback` (English). Each set names tn, twl, tw, tA, optional tQ and simplified text (D64),
  and OBS members (D74, D75) (`DEC:72-79`).
- Resolution unit (tool, book); the ladder has exactly two rungs; any other language is an
  explicit whole-project gateway change. Per-check language mixing inside a book never happens
  (D30(1–2), `DEC:291-298`).
- The project's pins bind every opener; a personal preference never changes the project's
  resources (D30(3)). This retires tC3's v52/v53 reviewer-drift failure (`DEC:298-300`).
- The resolved resource is recorded in each §5.2 decision file; a resolution change is a warned
  update (D17).
- Cross-language re-attach key: reference + original-language quote + occurrence. Not unique:
  duplicate rows exist in en_tn 2TI and ACT. Tiebreak tn by `SupportReference`, tw by the TWLink
  slug; still ambiguous → unplaced for review (`DEC:66-71`).

**Identity is the commit sha (D58, D59, D82).**
- A pin entry requires `sha` (40 lowercase hex); `version` is an optional display label, never
  compared (`DEC:977-981`). Reason: tags are not enforced upstream; the sample's `en_ugl v2` and
  `en_uhl v1` tags did not exist (`DEC:970-973`).
- Fetching still uses tags (`/sb/<tag>.zip`); the export's recorded revision is verified against
  the pin after arrival (D23(b), `DEC:982-983`).
- Import: a tag resolves to a sha by one DCS tag lookup; every version-identity fallback in the
  app was deleted, not guarded (`DEC:1012-1016`). A tC3 import stores no pin without a sha
  (D82, `DEC:1796-1826`).
- Sha-only resources with no tag or sb-zip (the lexicons, the OBS picture pack) are fetched as the
  Gitea commit archive `/archive/<sha>.zip`, verified against the archive comment
  (D71, `DEC:1336-1339`; `PKG:932-938`).

**The resource is the primary key (D36).** When a resource changes, the check list derived from
the new pin is the work. A stored decision that neither re-attach pass can place is marked
`invalidated: true` + `status: "invalid"`, kept in full (re-pinning the old resource re-attaches
it), and not counted. A book that was 100% checked is honestly less than 100% after the change
(`DEC:394-400`). The reconciliation runs before the pins move, under compare-and-swap, and the
dialog states "N carried over, M to check again" (`DEC:401-405`).

**Coverage records (D41, D61, D62).** Record which books a pin covers, so "primary not installed"
can be told apart from "primary does not cover this book". Coverage may widen from a sha-exact
local read and never shrink (`DEC:1050-1052`). Whole-collection resources record `["BIBLE"]`
(`DEC:1064-1067`).

**Upgrades (D72(5)).** On-demand "Check for updates", online only. One offer per language set,
each upgraded in its own confirmed step with the D36 carry-over report. Pins move only after
every resource of the release is installed and sha-verified, so a failed download changes
nothing (`DEC:1390-1396`). Original-language upgrades invalidate and retain alignments, never
re-key.

**The TW/TWL saga, the most expensive lesson (D32 → D34 → D96 → D97).**
1. D32: import tw and twl as separate burritos (`DEC:316-332`).
2. D34: the `en_tw` sb-zip export carries both halves, so pin one repo for both slots
   (`DEC:353-369`).
3. D96: false. DCS builds `/sb/v91.zip` on each request and takes the TWL TSVs from `en_twl`
   `master`, not from a release. tC4 shipped TWL for 7 books that no `en_twl` release contains,
   and two builds with the same pin could ship different TWL rows (`DEC:2539-2546`).
4. D97: the one-repo form is illegal; each slot pins its own repo and sha (`DEC:2585-2591`).
5. PACKAGING still records that "two builds of one pin can ship different bytes" for the `en_tw`
   export (`PKG:961-964`).
Lesson: a server-generated export is not a content-addressed artifact. Verify that the bytes
behind a pin are reproducible before trusting a pin.

**Data quality.** D100: `ar_gt/ar_tw` v3 held 420 Persian articles out of 422 under the tag `ar`;
`ar_tn` v3.2 held one book (`DEC:2742-2745`). Two-gateway evidence: a 2024 es-419 export names its
org as `Idiomas-Puentes`, which was renamed and 404s (`ev/two-gateway-path-2026-08-04.md`). D37:
DCS paths differ in case (`Es-419_gl` vs `es-419_gl`); store DCS's form and compare
case-insensitively (`DEC:413-424`).

**Bundled suite (PKG:908-1003).** Sixteen repos ship in every installer: the eleven-resource
English Bible suite (ULT, UST, UGNT, UHB, tN, TWL, TW, tA, tQ, plus the two lexicons), four OBS
resources and the 49.1 MB picture pack. Each distinct revision installs atomically into its own
full-SHA folder; existing installs and projects are never touched; startup verifies sizes and
checksums against `BUILD-MANIFEST.json` (`PKG:944-972`). A weekly CI job reports pins that fall
behind the Door43 catalog, with a negative control (`PKG:974-983`).

### 2.6 Versification

Decision trail: D23(e), D24(c), D25, D26, D40, D60.

**The rule.** Refs are stored in the project's chosen frame. TSV refs and original-language refs
are mapped into that frame at derive time (D24(c), `DEC:152-155`). Every project goes through the
same path; there is no eng-only design (D40, `DEC:487-500`). A same-frame project maps nothing and
validates nothing (D60, `DEC:1033-1039`).

**What the platform does (`ev/pankosmia-versification-2026-07-30.md`;
`ev/versification-format-and-frames-2026-08-24.md:162-192`).**
- Six schemes ship: `eng` (265 mapped entries, 126 of them ranges), `lxx` (with 74 partial
  verses), `org` (8 entries), `rsc`, `rso`, `vul` (`ev/pankosmia-versification-2026-07-30.md:20-33`).
- The server copies the chosen scheme to `ingredients/vrs.json`, reads only `maxVerses` to
  scaffold books, **discards the scheme name**, and never maps. "Versification is a template
  selector and a verse-count table" (`ev/versification-format-and-frames-2026-08-24.md:177-182`).
  The scheme name is unrecoverable from the project, so tC4 records it itself (`:184-192`, `:232-244`).
- The canonical book list comes from a hardcoded `eng.json`. Non-canonical books are not allowed
  (D26, `DEC:179-181`).

**Measured facts a rebuild must handle (`ev/versification-format-and-frames-2026-08-24.md`).**
- The format is a fork of the Copenhagen Alliance spec: `mappedVerses` values may be a string or
  an array. All shipped data uses strings. The Proskomma reverse map, given a string, iterates its
  characters and silently builds a corrupt table keyed `"0"`, `"1"`, … Normalize to arrays at the
  single load point (`:16-46`).
- Mapping pivots through `org` and is lossy. Mapping all 31,104 eng refs: `eng` composed loses 3
  verses; `vul` has 28 collisions and 171 refs outside the scheme; `lxx` 81 and 57 (`:55-75`).
  Results can be verse 0, past chapter end, in a missing chapter, or backwards across chapters.
  Validate every mapped result against the target `maxVerses` before it becomes an identity;
  otherwise drop it with a recorded reason, never journal it (`:96-100`).
- Composing unconditionally regresses the default project; return the ref unchanged when the
  frames match (`:102-109`).
- Every resource in the suite (helps and original-language texts) is in the `eng` frame. Jonah 1
  has 17 verses only in `eng`; all 12 published tN resources with Jonah reference `JON 1:17`
  (`:111-142`). User-chosen source texts can be in other frames (a Synodal-family Russian Bible
  fits `rsc`), but source panes are a lookup, not an identity (`:144-160`).
- en_tn v90 loss sweep: `eng` loses 0 of 84,158 rows; `rsc` 8; `rso` 13; `lxx` 81; `vul` 1,067
  (mostly Esther) (`:285-310`).
- The spec's `role: x-versification` was validated only by the harness's own generator; nothing
  else writes roles (`:219-230`).
- The mapping engine (`proskomma-core@0.11.3`) is a lazy 1,042.79 kB chunk (268.47 kB gzip),
  never loaded for a same-frame project (`:261-283`).

**For a Rust rebuild.** The vrs JSON data and the mapping algorithm are small. Owning a native
mapper with the validation rules above is simpler than embedding Proskomma. Keep the measured
sweeps as fixtures.

### 2.7 Graphite, script rendering and the desktop shell (D20)

**The requirement.** D20 (2026-07-19, owner directive): the shell is Electronite, "a hard
requirement so complex-script languages needing SIL Graphite smart-font rendering work
throughout the app; plain Electron/Chromium does not shape Graphite-only fonts" (`DEC:105-112`).

**Proof (`ev/graphite-shaping-packaged-2026-08-14.md`).** The probe font was Awami Nastaliq, which
has no OpenType tables. Same text, font, server and 48px size:

| Engine | Width | Ascent / descent | Rendering |
|---|---|---|---|
| Packaged Electronite (Chrome 138, Electron 37.1.0-graphite) | 390.06 px | 86.34 / 16.34 | joined, stacked Nastaliq |
| Plain Chromium 151 | 770.32 px | 58.38 / 25.99 | isolated, unjoined letters |
| Arial control | 411.75 px both | identical | — |

(`:49-58`). The proof covered macOS arm64 only; other OS artifacts use the same Electronite
release but are unmeasured (`:75-76`).

**Shell facts that cost time.**
- The pristine Electronite v37.1.0-graphite macOS release fails `codesign --verify --deep
  --strict`. A quarantined download got "Electron is damaged", with no Open Anyway. The build now
  re-seals the bundle (`ev/desktop-artifact-witness-2026-08-25.md:8-19`).
- The desktop template downloads Electronite in its install scripts; there is no Electron npm
  dependency (`ev/desktop-packaging-spike-2026-08-14.md:9-19`).
- The packaged startup patches the template in place: singleton lock, resource binding, window
  frame, isolated store. Each patch refuses to run if the template's shape changed (`PKG:126-132`,
  `PKG:147-155`).
- Two resource selectors in the server (launch `APP_RESOURCES_DIR` and the saved
  `app_resources_dir`) let a stale older install supply templates; a Windows OBS story arrived
  with CRLF bytes from an alpha.6 tree. The fix binds both to the current package's `lib/`
  (`ev/packaged-resource-binding-2026-09-21.md:206-212`; `PKG:241-262`).
- Fonts ship inside the app since #3. Before that, Google CDN fonts broke offline use (D38,
  `DEC:446-448`). Offline screens use Mulish, Charis SIL, Noto Serif, Noto Serif Hebrew and Amiri
  (`PKG:884-886`).

**For a Rust rebuild (my inference, not in the docs).** D20 is a rendering requirement, not an
Electron requirement. Options and the Graphite question each raises:
- Tauri uses the system webview: WebView2 (Chromium) on Windows, WKWebView on macOS, WebKitGTK on
  Linux. None is known to shape Graphite. A Tauri shell would likely fail D20.
- Keep Electronite (or another Graphite-enabled Chromium) as the shell over a Rust core. This keeps
  D20 and the PDF route.
- A native Rust UI must shape text with HarfBuzz built with graphite2 (`hb-graphite2`).
  `rustybuzz` has no Graphite shaper.
Run the Awami Nastaliq probe as the acceptance test for any choice. It discriminates by width.

### 2.8 PDF

Decision trail: D18 (print-CSS lead), D79(3–4), D80(1, 5); `ev/pdf-bridge-2026-09-24.md`.

- **Route.** The print-styled route: "the same Chromium renders the editor and the PDF"
  (D79(3), `DEC:1618-1622`). Graphite shaping therefore applies to the PDF too.
- **Rejected.** The Pankosmia PDF publisher (paged.js rendered by headless Firefox through
  puppeteer, Firefox downloaded at run time), because "a run-time browser download is
  incompatible with the offline pilot" (`DEC:1619-1621`). Its repository has no licence, so tC4
  copies no file from it (`DEC:1745-1747`).
- **Mechanism.** `window.print()` makes a file only through the print dialog. The chosen bridge:
  `ipcMain.handle('export:pdf', html)` writes a temporary HTML file (not a `data:` URL, which
  Chromium limits to 2 MB), loads it in a hidden window, calls
  `printToPDF({ preferCSSPageSize: true })`, returns the bytes and deletes the file
  (`ev/pdf-bridge-2026-09-24.md:53-75`). Joplin, MarkText and Zettlr do the same (`:65-72`).
- **Numbers.** 122–346 ms per document; Titus A4 single column is 1 page, about 40 KB; CSS
  `@page { size }` sets the paper; RTL pointed Hebrew renders correctly with columns, drop caps
  and verse numbers mirrored (`:46-51`, `:92-97`, `:101-111`). A Psalms-sized book paginates in
  the preview in 649 ms into 100 sheets (`:31-35`).
- **Rules.** Chapters with no drafted verse are left out; an undrafted verse inside a shown chapter
  prints `[ verse not yet drafted ]`; a book with nothing drafted refuses with
  `export.nothing-drafted` (`:121-126`). Page setup stays in memory (D80(5)). Two OBS layouts are
  planned (D79(11)).
- **Limits.** The print document loads no web font, so the PDF uses the machine's installed fonts
  (the rc.1 offline PDF embedded PT Serif only, `ev/offline-run-2026-10-02.md:69`). Page counts can
  differ between machines. Only macOS was measured (`ev/pdf-bridge-2026-09-24.md:148-156`).

**For a Rust rebuild.** If the shell stays Chromium-based, keep `printToPDF`. Otherwise the
rebuild needs a PDF engine that shapes Graphite (for example a typesetting pipeline on HarfBuzz +
graphite2), and it should embed the project's fonts rather than system fonts.

### 2.9 Packaging and installers

Decision trail: D39 (single instance), D46 (versions), D49 (store), D89 (ship rules), D99
(Windows signing); `docs/PACKAGING.md`.

**Pipeline (`PKG:14-51`).** One script, `scripts/package-desktop.zsh`, on three runners
(`macos-15`, `ubuntu-24.04`, `windows-2025`; Windows under MSYS2 zsh). It builds the client, builds
the pinned server from source, clones pinned read-only inputs (template, resource-core,
webfonts-core), stages Electronite + app + licences + `BUILD-MANIFEST.json` (every input with
version, commit, checksum), and smoke-tests through the shipped entry point (`PKG:269-316`).
Uploads are single-layer (`actions/upload-artifact@v7` with `archive: false`) because the action's
own archive resets file modes to 644 and testers had to unzip twice (`PKG:31-48`).

**Artifacts.**
- macOS arm64: unsigned `.pkg` (ad-hoc signed app, not notarized) plus zip; debug zip. macOS x64
  is not built (#44).
- Windows x64: Inno Setup per-user installer (`%LOCALAPPDATA%\Programs`, no elevation) plus zip.
- Linux x64: zip only, run from a terminal; no installer, no desktop entry; drops the Chromium
  sandbox when `chrome-sandbox` is not setuid root (`PKG:438-459`, `PKG:1043-1047`).
- Uninstall keeps projects on macOS and Windows (`PKG:348-349`, `PKG:660-662`).

**Guards, not conventions.** The smoke test reads the booted app and fails the build when:
the store resolves to `pankosmia_repos` or production is not empty at first boot (`PKG:97-104`); a
second launch survives or starts a second server (`PKG:133-136`); `/api/list-clients` lists more
than `uw-tc4` (`PKG:304-306`); `/api/version` disagrees with the staged `product.json`; the zip
holds two `product.json` files or files from two builds (`PKG:310-316`). Each guard has a forced
failure path (`TC4_TEST_FORCE_SHARED_STORE=1`, `PKG:102-104`).

**Two smoke tests (`PKG:754-764`).** Build-time (can this artifact start on the build host?) and
post-install (does the installed app work?): first start under a poisoned resource selector,
offline OBS image decode with the net gate off, project create, verse write, restart, read back,
server ZIP export compared with the raw metadata route, an upgrade leg with old pins preserved.

**Signing.**
- macOS: unsigned, ad-hoc, not notarized (D89(4)). Witnessed problems: Open Anyway did not start
  Installer for the quarantined pkg; `xattr -d com.apple.quarantine` was needed; after that,
  install took 12.5 s and first launch had no prompt (`ev/macos-launch-witness-2026-09-10.md:16-29`).
  The clean macOS 15 witness is still pending (`PKG:354-355`).
- Windows: signed from v4.0.0 (D99). Azure Artifact Signing, GitHub OIDC (no stored secret),
  only on manual runs on `main` or `release/*`. Every unsigned `.exe`, `.dll` and `.node` is
  signed, with an RFC 3161 timestamp because the certificate lives three days (`PKG:541-629`).
  Cause: on a machine with App & browser control on and Smart App Control off, the unsigned app
  ran with no window and no warning; signing two graphics DLLs fixed it
  (`ev/windows-unsigned-silent-start-2026-10-13.md`).
- Linux: unsigned zip.

**Windows-specific failures, each found only on a clean machine.**
- `server.exe` needs `VCRUNTIME140.dll`; the CI image already has the redistributable, so CI
  could not see the failure. The build now stages the CRT DLLs beside `server.exe` and refuses to
  package without them (`PKG:637-652`; `ev/offline-run-2026-09-14.md:119-145`).
- A not-found read on Windows has a different error text; the client's absence check read only the
  Unix text and showed an error dialog at project creation (#228,
  `ev/desktop-windows-witness-2026-09-08.md`).
- The native title bar and menu row showed until #639 (`PKG:142-213`).
- Chromium on Windows: one HTTP request costs about 4 ms direct and 13.6 ms through the Vite proxy
  (see §5).

**Release mechanics.** One release branch per minor (`release/4.0` cut 2026-10-13 at `17f4cbc`);
fixes merge to `main` first and are cherry-picked with `-x` (`PKG:1019-1036`). v4.0.0 promotes a
tested rc artifact and does not rebuild it (D89(1); D99(6) moves this to rc.3).

**Sizes.** See §5.

### 2.10 Offline and internet consent

**Commitment.** tC4 is built for offline field use (`PKG:835`). D7: no telemetry. D30(5): offline
with a missing pinned version is a first-class unavailable state that blocks nothing else.

**The consent model changed three times in eight days.**
1. D86 (2026-09-30): a persistent "Internet" / "Local" status, stored across launches; default
   Local (`DEC:1955-1983`).
2. D88 (2026-10-02): an account menu; a "Use the internet?" dialog for each task while "Ask before
   using the internet" is on; consent separate from the platform net gate (`DEC:2093-2136`).
3. D95 (2026-10-07): one on/off state for the app session, off at every launch, nothing stored.
   A step that needs the internet asks in place; "Turn on internet" lasts the session; the account
   menu turns it off (`DEC:2457-2489`). Reason: D88's per-task consent was built as a count of
   open "holds" across `await`s; PR #540 went through seven review rounds, each finding a new way
   a hold leaked (`DEC:2444-2449`). "One state with no temporary answers has nothing to take back."

**What stands (D88(5), D95(4)).** Startup, idle time, local import, and translation and checking
with installed resources make zero external requests. `guardFetch` and the Door43 adapter refuse
every request to another origin, and to `/api/gitea/` and `/api/git/push/`, while the internet
is off, before the request is sent (`DEC:2476-2484`). The platform net gate is off at start and
read back after it is turned on.

**Proof.** A Playwright regression test records every request and WebSocket during open, draft
and restart and fails on any host outside the local server (`PKG:896-906`). The manual offline
run has passed on Linux at alpha.4, alpha.5, alpha.6, alpha.7 and rc.1, in a loopback-only network
namespace (`unshare -rn`). macOS and Windows have not had a full offline run; Windows Sandbox runs
with networking disabled cover the app language and the clean-machine smoke
(`ev/windows-sandbox-522-2026-10-08.md`, `ev/windows-clean-machine-546-2026-10-09.md`).

**Exceptions accepted.** "Report a problem" sends through SendGrid with a token built into the app;
"anyone can extract a token that ships in an app. The owner accepts this exposure, as tC3 did"
(D89(10), `DEC:2215-2223`). Help pages are pinned (`tc-website` commit) and bundled, and open with
zero requests (D89(9)).

**For a Rust rebuild.** Put the network boundary in the Rust core: one HTTP client, one switch, an
allowlist of hosts. That is simpler than guarding a browser's fetch.

---

## 3. The risk ledger (`docs/RISKS.md`)

Ten rows, numbering stable; no row has been retired in the Mitigation column.

| # | Risk | Mitigation | Rust rebuild |
|---|---|---|---|
| 1 | Upstream release cadence against the server pin (0.18.15 rev `a83725b`) | Re-run transport and round-trip suites at each pin move, with a control on the old pin | Disappears if the rebuild owns the server |
| 2 | Two writers on one book file in Phase 1 | Single app, single user; load-time revalidation self-heals; Phase 2 solves it | Single instance + in-process lock still needed |
| 3 | Load-time derivation cost on large books | Measure first (OQ #9); optional cache keyed by content hash | Measured and closed: 20.6 ms worst case (D35); the row was never marked retired |
| 4 | Phase 2 scope creep | Phase gate; option to journal checking first | Process |
| 5 | A share-born `main` is not proven equivalent to an integrate-born team main | X8 scenario before any sync feature; rebuild `main` from the share history if needed | D98 makes Door43 `main` the team main; the proof obligation still applies |
| 6 | Two devices before team sync: the next share is not a fast-forward | Refuse `share.non-fast-forward`; never force push | Keep |
| 7 | Door43 repo name collision | Create first; refuse `share.name-exists` before any push | Keep |
| 8 | Door43 tooling reading `checking/` sidecars and the journal | Door43 Preview ignores unknown ingredients; journal paths unverified; re-check at first pilot share | Keep |
| 9 | The platform's https push panics on a missing username or token | Validate both before the call; route upstream | Pankosmia-only |
| 10 | An older build reads a newer journal envelope as damaged (v4.0.0 accepts `v` 1 and 2 only) | Ship a newer-version refusal before the first `v: 3` writer; rollback plan #46 in 4.0.1 | Keep: version the envelope and refuse "newer" distinctly from "damaged" from day one |

Gaps in the ledger: TEAM-SYNC-PLAN S9 and LEGIBILITY L-7 say rows should be added for journal
scale, identity store loss, power-loss durability and sync data loss, each naming its check
(`plans/TEAM-SYNC-PLAN.md:458-461`; `plans/LEGIBILITY.md:130`). Those rows do not exist yet, although
the evidence (§5) shows journal scale is the largest measured risk.

---

## 4. Roadmap: built, not built, and the pilot

**Plan of record.** `docs/ROADMAP.md`, ratified 2026-08-12 (D42, D46, D47); Increment 7 inserted
2026-09-15 (D74). The ordering principle is data first: work that accrues bad data if postponed
lands first (`ROADMAP.md:22-32`). Note that the "What has shipped" section lists only alpha.1
and alpha.2 and is stale; the evidence files record later builds.

**Shipped pre-releases (from DECISIONS and evidence).**

| Build | Date | Increment | Content |
|---|---|---|---|
| v4.0.0-alpha.1 | 2026-08-10 | 1 | Create a project, choose sources, draft beside them (J1–J2) |
| v4.0.0-alpha.2 | 2026-08-10 | 2 | Pinned resources, check a book, align verses (J3–J6, J13); eng-proven only (D40) |
| alpha.3 | ~2026-08-29 | 3 | Data foundations: versification mapping, version stamps, coverage records, CI format guardians, §8 normative, write-only journal, packaging pipeline, single instance, Graphite proof |
| alpha.4 | 2026-09-06 | 4 | Open, resume, multi-project; English suite bundled (#163) |
| alpha.5 | 2026-09-09 | 5 | Section drafting, verse spans, formatting, async save, Windows build, lexicons |
| alpha.6 | 2026-09-14 | 6 | Alignment suggestions, check comments and bookmarks, resource upgrade (J12), guided fix, fold in a worker |
| alpha.7 | 2026-09-22 | 7 | Open Bible Stories: create, translate frame by frame, understand, check (J20–J22, J25) |
| rc.1 | 2026-10-02/03 | 8 + 8.5 | Export (PDF, aligned USFM, plain USFM, Burrito zip), import (tC3 zip, USFM, Burrito) as new project, OBS export, first share to Door43, account menu |
| rc.2 | pending | 9 | D99: "rc.2 tags when the Arabic work is ready", Windows unsigned |
| rc.3 | pending | 9 | Signed Windows; v4.0.0 promotes the rc.3 artifacts (D99(6)) |

**Pilot release v4.0.0, 2026-10-16** (`ROADMAP.md:67-75`; D89; D99). "v4.0.0 is a pilot release":
selected external users plus all internal users; "4.0.0 is a single-translator release"
(`ROADMAP.md:83`). D89 says a red gate moves the date; D99 makes Windows signing a gate. As of
2026-10-14 the date is therefore conditional on the signed rc.3. The pilot program (#58) runs in
parallel and does not block the release (`DEC:2230`).

What the pilot includes:
- Bible projects: create (wizard; regional tags kept whole), section drafting with verse spans,
  Understand (helps), Translate, Check (tN, tW with check comments and bookmarks), Align (manual,
  plus suggestions learned from the project, off by default).
- OBS projects: create, translate frame by frame, understand, check.
- Resources: bundled English suite (16 repos including OBS and the picture pack), gateway-language
  download, two-rung fallback, upgrade flow (J12), guided fix.
- Export (J7, J23): dated PDF (print route), aligned USFM, plain USFM, Scripture Burrito zip.
- Import (J9a, J9c, J9d): tC3 zip, raw USFM, Scripture Burrito, each as a new project.
- Share (J11, J24): send-only push of the working `main` to Door43, token optionally in the OS
  keychain.
- The journal write side for every mutation, from day one.
- Account menu with the session internet switch (D95), offline help pages, Report a problem,
  App language (four catalogs incl. Hindi, `ev/windows-sandbox-522-2026-10-08.md`), keyboard tab
  order and focus ring on Check pages only (D89(8)).
- Installers: macOS pkg (unsigned), Windows exe (signed), Linux zip.

What is not built or deliberately after 4.0.0:
- Team sync (receive, integrate, conflict review): 4.1.0 round 1 (D98, epic #24). The spec rules
  R-8.7.7–R-8.7.12, the name action and JC-35 have landed (`DEC:2689-2692`); the app work is
  issues #674–#681.
- 4.1.0 also: Arabic support (#658), import into an existing project (#365), the renumber editor
  action (#364), book introductions (#55), Home check percent (#139) (`ROADMAP.md:64`).
- 4.2.0: BT Servant chatbot (#651).
- 4.0.1: the rollback plan (#46), the usfm-js / word-aligner upgrade (#420; until then an aligned
  word loses its `\add` style in aligned USFM export), Windows signing was moved back into 4.0.0.
- After 4.0.0, unscheduled: RTL rendering proof (#29), full screen-reader pass, i18n key audit
  (#12), auto-merge of concurrent edits (#27), OBS wrapped layout (#11), OBS audio (#294), CSV
  export (#367), automatic update check (#259), history view (#78), macOS signing and
  notarization, macOS x64, Linux installer, Draft/Align keyboard pass.
- Open technical residuals: whole-Bible open time (minutes), per-rule mutation hardening (D55),
  three-device fold generation, power-loss durability, the clean macOS 15 install witness, the
  rc.2 Windows clean-machine witness, Linux single-layer clean-machine witness.

Inconsistency to note: `ROADMAP.md:73-74` and `:81-82` still say OBS export and Door43 send for OBS
are not in 4.0.0. D79(2) corrected that: OBS export ships in Increment 8 and J24 rides J11
(`DEC:1615-1617`, `DEC:1666`).

---

## 5. Lessons from the evidence files

### 5.1 Measured numbers

**Derive (TSV → check items), `ev/derive-perf-2026-08-03.md:9-17`.** Apple silicon, median of 5.

| Tool | Book | File | Items | Median |
|---|---|---|---|---|
| tN | TIT | 56 KiB | 157 | 0.4 ms |
| tN | MAT | 1,987 KiB | 5,246 | 12.9 ms |
| tN | PSA (worst) | 3,041 KiB | 7,971 | 20.6 ms |
| tW | PSA | 401 KiB | 7,683 | 6.2 ms |

About 6.8 µs per KiB. Conclusion: no derive cache (D35).

**Fold and open, `ev/bench-fold-2026-08-25.md`, `ev/fold-constants-92-2026-08-25.md`.** M2 Pro,
Node 22. Synthetic corpora from real material: aligned NT = 15,945 events; checked whole Bible =
278,041 events (215,767 decisions).

| Measurement | Before #92 | After #92 |
|---|---|---|
| Fold, aligned NT | 671 ms | 329 ms (−51%) |
| Fold, checked whole Bible | 9,279 ms (13,737 ms after hours of benchmarking) | 5,641 ms back-to-back (−59%); 4,239 ms in one cool run |
| Reference reader scan + fold, NT | 1,652 ms | 1,393 ms |
| Reference reader scan + fold, whole Bible | 41,258 ms | 42,827 ms (I/O variance) |
| HTTP read per segment (rig) | 0.319 ms | — |
| Projected production open, whole Bible | 3–4 minutes (two scans) | — |

(`ev/bench-fold-2026-08-25.md:37-48`, `:93-107`; `ev/fold-constants-92-2026-08-25.md:38-53`.)

- Hotspots: per-comparison canonicalization in the decision sort (~3.5 s; 7.6M serializations),
  per-alignment-head slot-list recomputation (~2 s), per-fold re-validation of all events (~1.8 s).
  The usfm-js parse, presumed dominant, cost 112 ms for 31k parses (`ev/bench-fold-2026-08-25.md:127-138`, `:279-284`).
- Absolute fold times drift with machine state; only back-to-back ratios are stable claims
  (`ev/fold-constants-92-2026-08-25.md:9-13`).
- Every save re-folded the whole journal on the UI thread: a ~9.3 s freeze per save at whole-Bible
  scale (`ev/bench-fold-2026-08-25.md:149-164`).

**Fold in a worker, `ev/fold-worker-2026-09-14.md:31-52`.** Titus with 40,000 edits (40,002
segments). Main-thread fold: 222 ms and 213 ms long tasks per save, 214 ms input stall. Worker: no
long task, 31 ms and 10 ms stalls. The fold cost did not change. Opening that project took 81–90 s.

**Production open, `ev/open-time-2026-09-05.md`, `ev/open-time-run2-2026-09-05.md`.** 4,002
segments (2,044,187 bytes, average 510 B), Vite dev client on the rig, macOS: 5,135 ms and
4,620 ms click-to-text, about 1.15–1.28 ms per segment all-in, with a determinate progress
indicator (`open-time-run2:31-43`). The owner's criterion was "a slow open shows real progress, not
a speed target". Linear extrapolation: about 20 s for an aligned NT, about 6 minutes for a checked
whole Bible (`open-time-2026-09-05.md:60-67`).

**Windows open, `ev/open-time-windows-2026-09-28.md:19-58`.** Per-request cost of one segment read:

| Client → route | ms per request |
|---|---|
| Node fetch → rig | 0.89 |
| Chromium fetch → rig | 4.12 |
| Chromium fetch → Vite proxy → rig | 13.62 |
| Same with a keep-alive proxy agent | 3.17 |

1,004 requests in 23.7 s; the 4,002-segment open needed over 80 s. Fix: read the whole journal as
one zip through the ingredient-directory route: 2,258,632 bytes in about 1.3 s, one request
instead of 4,002. Home also queued hundreds of reads for OBS progress on a six-connection pool.

**Startup and bundle.**
- Cold start of the production client to interactive: 334 ms median (`ev/bench-fold-2026-08-25.md:196-211`).
- Client `dist/` 2,233,128 bytes, JS gzip ≈ 580 kB, two chunks over 500 kB, no code splitting
  (2026-08-25, `:181-194`).
- With the suggestion engine: `dist/` 3,952 kB; worker chunk 279.55 kB; main chunks 1,042.79 kB +
  1,082.25 kB (`PKG:1007-1017`).
- Proskomma mapping chunk 1,042.79 kB (268.47 kB gzip), lazy.

**Installer and artifact sizes.**

| Artifact | Bytes |
|---|---|
| macOS arm64 zip, alpha.3 (no resources) | 142,556,909 |
| macOS zip with English suite (alpha.4) / with lexicons | 173,313,348 / 178,591,597 |
| Linux zip with suite / lexicons | 180,227,460 / 185,618,191 |
| Windows zip with suite / lexicons | 188,345,159 / 193,731,895 |
| alpha.7 (OBS + 49.1 MB picture pack): Linux zip / macOS pkg / macOS zip / Windows exe / Windows zip | 223,955,557 / 213,636,234 / 221,926,764 / 162,889,153 / 232,181,903 |
| rc.1 Linux zip | 228,949,754 |
| rc.1 Windows zip / exe (with VC++ DLLs) | 259,009,238 / 186,525,858 |

(`PKG:995-1005`; `ev/offline-run-2026-09-22.md:18-24`; `ev/offline-run-2026-10-02.md:12-14`;
`ev/windows-clean-machine-546-2026-10-09.md`.) The bulk is Electronite (Chromium) plus
resources; the client is about 4 MB.

**Other numbers.**
- Server build with MSVC: 7m14s uncached (`PKG:750-751`).
- Real-Mac post-install smoke: 36.99 s wall clock (`ev/offline-run-2026-09-14.md:50`).
- macOS pkg install after clearing quarantine: 12.5 s (`ev/macos-launch-witness-2026-09-10.md:19`).
- Alignment suggestions, training (M2 Pro): NT 7,958 verses in memory, booster fit on 206 verses,
  273.3 s; OT (GEN + PSA) 136.8 s; one prediction 101 ms / 56 ms
  (`ev/align-suggestions-bench-2026-09-12.md:20-23`). JLBoost is 95% of training time; appending one
  verse to memory costs 0.14–0.16 ms (`ev/align-suggestions-growth-2026-10-02.md:39-52`).
- Recall: plain wordMAP memory reaches 0.679 at 7,657 NT verses and beats every booster from about
  2,500 verses; the booster helps only between about 10 and 1,000 verses
  (`ev/align-suggestions-growth-2026-10-02.md:87-132`). A ~70-line JLBoost patch made the fit 26x
  faster with better recall; it is upstream work, not shipped.

### 5.2 What failed, and what each failure teaches

**Wrong claims about the platform.**
- The zip round-trip "defect" (D21) used the wrong endpoint; withdrawn five days later (D22,
  `ev/zip-roundtrip-correction-2026-07-27.md`). The AI "took its hints from the code paths of the
  tC4 prototype, not from the general platform surface".
- "The write path preserves roles" was an instrument error: the baseline was captured after the
  roles had already been wiped (0 === 0) (`DEC:117-118`).
- The upstream mirror was 15 commits stale: "an old mirror looks exactly like an absent feature"
  (`ev/pankosmia-versification-2026-07-30.md:15-18`).
- "Demo seed projects" in a fresh install were the developer's own repos, through the shared
  store (`ev/desktop-packaging-spike-2026-08-14.md:64-76`).
- The `en_tw` export was assumed tag-stable; it is built per request from `master` (D96).
- An audit's key simulation (`receive-with-unsent.mjs`) was not retained, so the run is not
  repeatable (`DEC:1149-1151`).

**Process failures.**
- One giant change set: PR #75, 306 checks green, five blocking defects (D51).
- Hand-rolled save machinery: 22 review rounds (D65). Consent holds: 7 review rounds (D95).
- The internet model changed three times in eight days (D86 → D88 → D95).
- Documented counts: 47 of 325 commits were manifest refreshes (D77, D78).
- 1340 unit tests, of which 306 could not fail on a shipped defect (D81). CI does not run the
  Playwright journeys, so journey-only defects are found late.
- "Follow-ups" in a summary document were not done, including CDN fonts that broke offline (D38).
- Specification text that only the harness's generator satisfied (`role: x-versification`).

**Data and algorithm failures.**
- The reference reader crashed past ~100k events: `events.push(...readSegments())` exceeded the V8
  stack (`ev/bench-fold-2026-08-25.md:140-147`).
- Strict reference validation dropped 112 real tN rows (D60).
- A frozen partial coverage record silently substituted the fallback (D61).
- The Proskomma reverse map silently corrupts on string values (§2.6).
- D72 built literally retrained for minutes per saved verse (D93).
- "Word bank empty" was treated as "verse reviewed" (D73).
- D59's "marked-unresolved" pin state did not exist in the format (D82).
- `checkingLanguage` was written and never read (D91).

**Platform drift.**
- From pankosmia-web 0.18.10, every commit rewrites `metadata.json`; transport went 10/10 → 5/10
  at 0.18.15 until the harness was re-baselined
  (`ev/rig-rebaseline-0.18.15-2026-10-03.md:22-75`). Byte-for-byte import broke (D90(6)).
- 0.18.0 panicked if no client was registered at `/clients/<homepage>` (D27(a)).
- 0.18.14's OBS create route refused every `x-` code; 0.18.15 fixed it (D90).
- Create routes refused `es-419` until 0.18.14 (D80, D90(4)).
- Server boot panics when the working directory exists without first-boot files
  (`ev/store-isolation-2026-08-14.md`; `PKG:91-95`).
- The commit author is the OS account name, with no route to change it (D85).

**Packaging failures.** Invalid Electronite signature ("damaged"); unsigned pkg blocked even after
Open Anyway; VC++ runtime missing on clean Windows, masked by CI; Windows silent start when
unsigned; stale resource selector from an older install; Windows error-text differences; the
template launcher's missing single-instance lock; the server stopped twice on quit (#206);
double-zipped CI downloads. Each one surfaced only on a real or clean machine, not in CI.

---

## 6. The Automerge proof (D52; `ev/automerge-proof-2026-08-19.md`)

**Question.** Issue #77: should a proven open-source merge library replace the custom journal? The
bar was "overwhelming evidence": meet every requirement, remove a substantial part of the custom
code, and carry less long-term risk (`DEC:725-729`).

**What was surveyed.** 13 libraries that merge locally. Three real contenders were tested:
Automerge, Yjs, Loro. `y-crdt`/`yrs` is wire-compatible with Yjs and shares its data model and
failure. GunDB was rejected for correctness (timestamp LWW with no operation record); RxDB's
default conflict handler drops data; server-dependent products fail independent offline peers
(`DEC:732-738`; `ev/automerge-proof-2026-08-19.md:271-293`).

**What was built.** Automerge 3.4.1, Node 22, against the existing validator, fold, HLC, USFM
codec and the real sample burrito, unmodified (`:23-35`). The architecture that passed (`:37-55`):
1. The root document holds only `schemaVersion`, `projectId` and an `actions` map. Each key is the
   SHA-256 of a sealed tC4 action body; each value is an immutable string.
2. One immutable action per Automerge change. Deletes, overwrites, unknown root fields, actor
   mismatches and nested mutation are refused.
3. An Ed25519 actor manifest binds the tC4 actor id, signing key and Automerge actor id. Canonical
   JSON envelopes carry a checksum, signature, change hash and Base64; 4 MiB limit.
4. Intake applies the received causal closure to a clone, validates every Automerge operation and
   tC4 action, runs the full fold, and accepts nothing if any part is invalid.
5. Durable state is immutable signed bundles; `A.save` is only a rebuildable cache; no
   `automerge-repo` storage or compaction.

**The two hazards it reproduced (`:57-62`).**
- Concurrent whole-verse text rewrites merged as Automerge text produced the novel string
  `MaryRut rewrot thedewhlendventse rewrote it.` with no conflict.
- Deleting a nested verse while another peer edited it hid the edit from the current projection
  with no conflict.
So Automerge MUST NOT merge verse text or hold mutable nested project state (`DEC:740-745`).

**Results.** All 15 cases passed (`:80-92`): concurrent verse candidates kept as a fork;
send-without-receive; exact TIT and JON rebuilds; hostile intake (a bad ancestor plus a dependent
honest change rejected as one quarantined closure, accepted bytes unchanged); readable history;
200 delivery permutations converge; immutable replay; two crash points; 8,640 bit flips and 1,080
truncations rejected (raw `applyChanges` accepted 77 of 1,040 bit flips into unexpected state);
three Automerge versions load the same document; a USFM fallback. The journal suite passed
336/336 and normative coverage 72/72.

**Correction of the first ruling.** The first D52 said a CRDT cannot refuse a bad change after a
peer builds on it. True for already-accepted history; false for unaccepted intake, which can be
quarantined as a whole causal closure (`:117-133`). Also "Automerge compaction loses history" was
wrong: `A.save` keeps the full change history; `automerge-repo` compaction only rewrites storage
(`DEC:756-762`).

**Costs that decided it.**

| Measurement | Constrained Automerge | Custom segments |
|---|---:|---:|
| Author/seal 50,001 actions | 418,420.64 ms (119.5/s) | 899.24 ms (55,602/s) |
| Fold | 325 ms | 246.18 ms |
| Receive, grouped | 45,510 ms in 13 batches | not a separate step |
| Raw envelopes | 46,952,687 B | 23,481,271 B |
| ZIP | 15,756,423 B | 18,905,925 B |
| Heap | 174 MB | 80 MB |

(`:194-203`.) Packaging: a 3,571.26 kB WASM file (1,128.42 kB gzip) plus 95 kB of JS; +38.26 ms
median cold-start wall time over 20 headless Chromium contexts (`:135-165`). Archive size was not a
reason to reject. Long-history authoring was superlinear (6 min 58 s for 50,000 edits); sharding
would add the lifecycle rules the replacement was meant to remove (`:205-211`).

**Code deletion.** None of substance. The custom journal was 2,688 lines; the safe Automerge path
added 415 lines (`model.mjs` 289, `storage.mjs` 95, `fallback.mjs` 31) that still call
`sealAction`, `validateAction` and `fold`. A browser version would also need WebCrypto key
enrollment, revocation and IndexedDB durability (`:213-226`).

**Carriage.** Four canonical JSON artifacts, including a 2,512,429-byte signed bundle, survived
the Pankosmia raw-ingredient API, metadata rebuild, git commit, ZIP export and import byte for
byte (`:228-251`).

**Ruling.** "Safety proof: PASS. Adoption-value test: FAIL. Burrito carriage: PASS" (`:5`). The
custom journal is retained. Re-opening needs new executed evidence that a library deletes
substantial project-owned code, preserves hostile-intake quarantine and permanent authored history,
and has bounded long-history performance (`DEC:788-795`).

**For a Rust rebuild.** The cost side was measured on the JS/WASM build. Native `automerge` (Rust)
removes the WASM size and start-up cost and may change the authoring curve. The value side does
not change: the domain grammar, fold, USFM codec, signatures and quarantine still have to be
owned. The proof's two hazards (text splice, hidden nested edit) apply to any CRDT. The
constrained "append-only set of immutable signed action bodies" model is in effect what the
custom journal already is, which is why the library added little.

---

## 7. The most consequential lessons for a Rust rebuild

1. **The journal is the data.** Append-only, per-actor, sealed, checksummed actions; every other
   file is a projection rebuilt by a deterministic fold; forks keep both versions; nothing is lost
   silently (D47(c), D48, D53, D55). Port the format and use the existing conformance suite as the
   oracle.
2. **Byte-strict edits.** Splice one verse; never re-serialize a book; a one-time seeder only for
   empty books (D8, D31).
3. **The resource is the primary key, and a pin's identity is a commit sha.** Unplaceable
   decisions are invalidated and retained, never deleted (D36, D58, D59).
4. **Verify reproducibility before pinning.** A server-generated export (DCS sb-zip of `en_tw`)
   changed bytes under a fixed tag (D96, D97).
5. **Versification maps once, at derive, with validation.** Same frame passes through; cross-frame
   results are checked against `maxVerses`; normalize mapping values to arrays (D24(c), D40, D60).
6. **Graphite is a rendering requirement.** Electronite satisfied it; a system-webview shell
   probably will not. Test with Awami Nastaliq (D20).
7. **Own the filesystem.** Most HTTP-boundary workarounds (IndexedDB outbox, no fsync, per-segment
   requests, metadata rewrites, role wipes) disappear with in-process storage (D28, D50, D90).
8. **Plan for journal scale.** One file per action reached 278k files and 1.1 GiB on disk; whole-
   Bible open took minutes. Pack segments, index heads, fold incrementally.
9. **Sync without git merges.** D98's round 1 (fetch, add journal files, refold, push, swap) with
   guarantees G1–G8 is the design to start from.
10. **Joining is not merging; a merge never blocks work** (D53).
11. **Single instance and an isolated store are enforced by tests** that fail the build (D39, D49).
12. **One session-scoped internet switch,** refused at the network boundary; zero requests at
   startup (D95).
13. **Sign Windows from day one** and witness every installer on a clean machine; CI runners mask
   missing runtimes and Gatekeeper behavior (D99).
14. **Format changes need a schema bump, a migration and old-data fixtures,** and a distinct
   "made by a newer version" refusal (D47(a), Risk 10).
15. **Green suites are not proof.** Every normative rule needs a check that fails when the rule is
   broken; generators must reach the states they claim; define journeys by on-disk end state
   (D51, D55, D69).
