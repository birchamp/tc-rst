# tC4 visual-parity baseline

Reproducible screenshots of translationCore 4 (tC4), the React client that the Rust rebuild must
look like. tC4 normally runs against a Pankosmia server on `127.0.0.1:19998`. That server is not
available here, so this directory holds a Node mock of the Pankosmia HTTP API, backed by
fixtures on disk. A Playwright script drives the real tC4 client against the mock and writes one
PNG per screen.

- tC4 source: commit **`d310bf304cfee784f4ca45dee474a3faf0d5616e`** (2026-10-16, `main`,
  `package.json` version `4.0.0-rc.1`, clean tree), built from a separate clone of
  `/home/user/unfoldingword/translationcore4` (see "Capture another tC4 commit").
- Reference version: tC4 4.0 has no tag yet. Until it has one, this commit is the reference
  (`docs/PLAN.md`, "The reference version"). When the v4.0.0 tag exists on `release/4.0`,
  capture again at the tag and record it here.
- The first baseline was captured at `d494a9d6ba9f4cdcc09faff9038c93159106045b` (2026-10-14).
  "Changes since d494a9d" lists the screens that changed. The old images of those screens are
  in `history/d494a9d/`.
- Help pages: tc-website `cdfdf9b7d534e7574beb3fcf44678973cdc5f9d7`, the commit that tC4 pins in
  `scripts/fetch-help.mjs`. The pin is the same at both commits.
- Output: 61 screenshots at device scale 1, in `screens/`, 7,998,133 bytes (7.6 MiB) in total.
  Each is 1280x800, except the six screens whose name ends in `-620`, which are 620x800. 49 are
  complete and 12 are incomplete. `manifest.json` lists each screenshot with the steps that
  reach it. The 42 screens of the first baseline were captured again; 19 screens were added for
  `d310bf3` ("Screens added for d310bf3").
- Browser: Chromium 141.0.7390.37, headless, through Playwright 1.56.1. This Playwright version
  matches the Chromium build 1194 in `/opt/pw-browsers`.
- Two consecutive full runs at `d310bf3` gave byte-identical PNGs, first for the 42 screens,
  then for 50, then for 59, and then for all 61. The last two runs also gave the same
  `manifest.json`. A control run at `d494a9d` on the same machine gave the committed `d494a9d`
  PNGs byte for byte, so each change listed below comes from tC4, not from the machine.

## Rerun

```sh
cd /home/user/tc-rst/parity/tc4-baseline
npm install          # playwright 1.56.1 and fflate 0.8.2, into ./node_modules (git-ignored)
npm run capture      # build the client if needed, rebuild the state, capture every screen
```

`npm run capture` does four things:

1. It builds the tC4 client into `.cache/tc4-dist/` once per tC4 commit, as `npm run build`
   builds it. First it runs tC4's prebuild step, `scripts/fetch-help.mjs`, which copies the
   pinned help pages from GitHub into tC4's git-ignored `public/help/`. Then it runs
   `vite build`. If tC4 has no `node_modules`, it runs `npm ci` there first. The result is the
   release build: base `/clients/uw-tc4/`, `import.meta.env.DEV` false, and Door43 at
   `git.door43.org`. No tracked file in tC4 changes. Without GitHub the build goes on without
   the help pages, and `manifest.json` says so in `client`.
2. For each scenario, it rebuilds `.state/<scenario>/` from the tC4 fixtures and
   `fixtures/vendor/` (`fixtures/build-state.mjs`).
3. It starts the mock server (`mock-server/server.mjs`) on port 19998. The mock serves the API at
   `/api/...` and the built client at `/clients/uw-tc4/`.
4. It drives each screen, writes `screens/<name>.png`, and writes `manifest.json`.

A full run took about 90 seconds after the first build at `d494a9d`. At `d310bf3`, with 61
screens on 4 CPUs shared with other work, it took about 5 minutes.

Options:

| Option | Effect |
|---|---|
| `--only home,en-check-tw-session` | Capture only these screens. Their entries in `manifest.json` are updated in place. |
| `--port 19999` or `TC4_BASELINE_PORT` | Use another port. The default port is the rig port, 19998. |
| `--rebuild` | Build the client again. |
| `TC4_ROOT=/path/to/translationcore4` | Use another tC4 checkout. |

### Check

```sh
npm run check        # or, from the repository root: node --test parity/tc4-baseline/baseline.test.mjs
```

The check needs no browser, no tC4 checkout and no network, and CI runs it. It fails when a
screenshot is not in `manifest.json` or has the wrong size, when a cell of this README's screen
table or a total differs from the manifest, or when the README does not record the tC4 commit.
It fails when a tool's marking row lacks a screen in English or Arabic at 1280 or 620 px.

It fails unless the screens show every part of the sync result toast: a failed sync with its
code, an ok sync with no change, "Changes from", a verse conflict with Review, and the note on
missing versions with Download. A screen whose toast counts missing versions must be
incomplete, and its reason must name each version. The capture lists those versions by tC4's
own rule (`missingPinCount` in `src/data/installed.ts`).

It fails when "Where the rebuild differs on purpose" differs from the manifest's
`rebuildDiffers` notes or cites a ruling that `docs/PLAN.md` does not list, and when a screen
shows a shared OBS card with "Upload changes" but does not cite Q2. The toast and the cards are
read from the DOM at each shot (`syncToast` and `homeCards` in `manifest.json`).

The check also starts the mock server and drives its sync routes and the fake Door43's branch
check (`mock-server/server.test.mjs`). Run it after every capture.

### Capture another tC4 commit

`capture.mjs` builds from `TC4_ROOT` and runs `git -C` in it. Do not check out another commit in
the shared clone. Make a separate clone instead:

```sh
S=/path/to/scratch
git clone --no-checkout --no-hardlinks /home/user/unfoldingword/translationcore4 $S/tc4
git -C $S/tc4 checkout d310bf3
ln -s /home/user/unfoldingword/translationcore4/node_modules $S/tc4/node_modules
mkdir -p $S/tc4/public && cp -a /home/user/unfoldingword/translationcore4/public/help $S/tc4/public/
TC4_ROOT=$S/tc4 npm run capture
```

The link to `node_modules` works only while the two commits have the same dependencies. From
`d494a9d` to `d310bf3`, `package.json` changed only its `test:desktop` script. The copy of
`public/help/` works only while the help-page pin is the same. Otherwise
`scripts/fetch-help.mjs` fetches the pages from GitHub.

To browse the mocked app by hand:

```sh
npm run client                                   # build .cache/tc4-dist
npm run state -- --scenario en-pins              # or: npm run state  (scenario "sample")
npm run serve -- --state .state/en-pins          # then open http://127.0.0.1:19998/
```

Writes go to the state directory. Run `npm run state` again to reset it.

To rebuild the vendored resource files, run `npm run vendor`. It needs github.com and
registry.npmjs.org, and it checks every download against a recorded sha256, so the files come
out byte for byte the same. The capture itself needs no network except the help-page fetch.

## What is in here

| Path | Purpose |
|---|---|
| `mock-server/server.mjs` | The Pankosmia API mock, backed by files on disk, plus the static server for the built client. |
| `mock-server/server.test.mjs` | Checks that the mock's push, clone, copy and delete answer as pankosmia-web does, and that the fake Door43's branch check agrees with the mock's Door43 (see "The mock server"). |
| `mock-server/fake-door43.mjs` | A fake Door43 (Gitea API v1) for the share and sync screens. It is routed inside the browser context. It is trimmed from tC4's own `e2e/helpers/door43.ts`. |
| `fixtures/build-state.mjs` | Builds a state directory, in the Pankosmia working-directory layout, from tC4's fixtures and `fixtures/vendor/`. |
| `fixtures/vendor/` | Resource files that tC4 does not vendor, cut from published copies by `make-vendor.mjs`. `README.md` and `SOURCES.json` there give the source of each file and the licence. |
| `capture.mjs` | The Playwright capture. It holds the screen list, the steps and the reason each incomplete screen is incomplete. |
| `baseline.test.mjs` | Checks that the screenshots, `manifest.json`, this README and the rulings in `docs/PLAN.md` agree (see "Check"). |
| `manifest.json` | Generated. Lists the tC4 commit, client build, browser, viewport, scenarios, installed resources (with substitutions), absent resources and every screen. |
| `screens/*.png` | Generated screenshots. |
| `history/d494a9d/` | The `d494a9d` images of the screens that changed, and that commit's `manifest.json`. Kept by hand, not by the capture. |
| `.cache/`, `.state/`, `node_modules/` | Regenerated by every run. Git-ignored. |

### The mock server

The mock implements every route that `src/data/serverApi.ts` wraps, under `/api`, with the
response shapes given there. Each route in `server.mjs` cites the line of its wrapper. The routes
that matter for the screens work against the state directory:

- Reads: `/version`, `/git/list-local-repos`, `/burrito/metadata/summaries` and `/summary`, and
  `/burrito/metadata/raw`. `/burrito/ingredient/raw`, `/bytes` and `/zipped` are served too. Also
  `/burrito/paths`, `/burrito/zipped`, `/content-utils/versifications` and `/versification/<name>`
  (from tC4's `test/fixtures/vrs`), `/client-settings/<id>`, and `/net/status`.
- Writes: `POST /burrito/ingredient/raw` keeps the `.bak` unless `no_bak` is present, and
  `update_ingredients` rescans `metadata.json`. `/burrito/ingredient/delete` renames the file to
  `.bak`. `/burrito/metadata/remake-ingredients`, `/client-settings`, `/net/enable`,
  `/net/disable` and `/app-state/current-project` are served too. tC4's journal, decisions and
  settings writes all land on disk, so the app behaves as it does on the rig.
- Errors and absent repos follow the pankosmia-web source in `/home/user/ref/pankosmia-web`
  (0.19.2, `bf0a5d3`; tC4 pins 0.18.15, whose source is not on this machine). A missing file
  answers HTTP 400 `could not read ingredient content: No such file or directory (os error 2)`,
  so tC4's `isNotFoundError` holds. A missing repo lists no paths, its metadata read answers
  `could not read metadata: …`, and its summary is the platform's `? Bad Metadata JSON ?` record.
  tC4 quotes some of these texts on screen, for example in the OBS picture notice.

These simplifications do not show on any captured screen:

- `/git/add-and-commit` records the message and clears the pending set, and `/git/status` lists
  the files written since then. `/git/remotes` reads `.remotes.json`.
- The routes that create a project or book, the pull and the zip imports answer
  `is_good: false`. The capture never submits them.
- The gitea catalog answers "offline mode" while the net gate is off, and `[]` when it is on.

Door43 `main` of each repository is a folder, `.state/<scenario>/door43/<host>/<org>/<repo>/`.
It holds the files of the last push. There is no git history. The sync routes follow
pankosmia-web's `push.rs`, `clone_repo.rs`, `copy_repo.rs` and `delete_repo.rs`:

- `/git/push` answers "offline mode" (HTTP 401) while the net gate is off. Otherwise it finds
  the remote by name and makes Door43 `main` equal to the project. The project's
  `.remotes.json` stays behind. No push is refused as not a fast-forward.
- `/git/clone-repo/<host>/<org>/<repo>` answers "offline mode" (HTTP 401) while the net gate
  is off. Otherwise it copies Door43 `main` to the fixed path `<host>/<org>/<repo>`, with the
  remote `origin` at `https://<host>/<org>/<repo>`. A folder there that is not empty answers
  HTTP 400 with `code=Exists (-4)`, which tC4 reads as `sync.clone-path-exists`. A repository
  that Door43 does not hold answers HTTP 400 `could not clone repo: …`. A clone whose
  connection drops answers HTTP 400 `could not clone repo: failed to connect to <host>;
  class=Os (2)`, the text of libgit2's failed connect (`streams/socket.c`) as git2 writes it.
  tC4 reads both as `sync.clone-failed`.
- `/git/copy` needs a source folder and a target that does not exist. It copies the folder,
  then deletes the source when `delete_src` is true. It has no net gate.
- `/git/delete` removes the folder. A folder that is not there answers the platform's HTTP 400
  `could not delete repo: No such file or directory (os error 2)`, which tC4 reads as "not
  there".
- `/git/branches` lists `main` as the checked-out branch.

Some steps change Door43 itself, not the app. The capture does them through `mock.door43` in
`server.mjs`: it pushes a project as its first share does, or it drops the connection of the
next clone. For teammates' changes, it adds journal segments to Door43 `main` with tC4's own
journal writers (`journal/files.mjs`), as tC4's `e2e/sync-button.spec.ts` does. These steps
start with "Door43" in the screen table.

`mock-server/fake-door43.mjs` also answers the check that Home makes before a sync
(`door43Api.ts` `hasBranch`). It reads the mock's Door43 folders: `main` answers 200 while
Door43 holds the repository, and 404 otherwise. On 404, tC4 skips the sync and opens the
Upload changes dialog (`state.jsx` `mainOnDoor43` and `syncsNow`). So every sync screen pushes
the project to Door43 first.

### Data: what the mock serves

The two projects are tC4's conformance samples, copied unchanged:

- `conformance/sample-burrito` becomes `_local_/_local_/sample_burrito`. It is "Equipo Ejemplo —
  Tito y Jonás", es-419, with Titus 1:1-5 drafted, two tN and two tW decisions, and one
  alignment (six links in Titus 1:1).
- `conformance/sample-burrito-obs` becomes `_local_/_local_/sample_burrito_obs`. It is the
  "Historias Bíblicas Abiertas" project, with story 1's title, frames 1 and 2, and reference
  drafted.

The sideloaded resources carry install records, as `dev-env/scripts/seed.zsh` and
`write-install-records.mjs` write them. Where a burrito's own `metadata.json` records an md5 for
a tC4 fixture, the build checks the file against it. Every resource is a Titus slice (or OBS).
"Substitute" means the content is not, or not only, the pinned release's export bytes;
`.state/<scenario>/provenance.json` lists every file with its source and both md5s.

| Resource | Pinned identity | Content served |
|---|---|---|
| en_ult | v91 `35d2159` | **Substitute:** v89 Titus (`84c73ba`, tC4 fixture) under the v91 identity. |
| en_ust | v91 `85f274a` | **Substitute:** v89 Titus (`37ec223`, tC4 fixture). The fixture has no metadata, so the en_ult v89 metadata, renamed, serves as its metadata. |
| el-x-koine_ugnt | v0.34 `fc95b2b` | **Rebuilt file of the pinned release:** UGNT v0.34 Titus from translationCore 3's bundle, written back to USFM (`fixtures/vendor/`). Every word agrees both ways with the ULT v89 alignment of Titus. The metadata is the en_ult v89 metadata, renamed. |
| en_tn | v91 `e586762` | The real v91 `TIT.tsv`. |
| en_twl | v91 `7cbb341` | The real v91 `TIT.tsv`. It is byte-identical (md5 `d3eaff3f`) to the copy that the en_tw v91 export carries. |
| en_tw | v91 `ff5b385` | **Partly substitute:** `TIT.tsv` and `kt/god.md` are the v91 bytes. The other 92 articles that the Titus words link are v88 (`fixtures/vendor/`); all but `kt/evil`, `kt/good`, `kt/save` and `other/courage` have the md5 that the v91 metadata records. |
| en_ta | v91 `ce9a1bb` | **Substitute:** the v86 export metadata, `translate/toc.yaml` and `figs-abstractnouns` (tC4 fixtures), and v88 for the other 30 translate articles that the Titus notes name (`fixtures/vendor/`). |
| es-419_tn | v66 `22f3d0c` | The real `TIT.tsv`. |
| en_obs | v9 `d39a1dc` | **Substitute:** story 1 only, from the npm package `open-bible-stories@0.0.5` (release unknown, `fixtures/vendor/`). The metadata is the en_obs-tn v13 metadata, renamed. |
| en_obs-tn, en_obs-twl, en_obs-tq | v13, v3, v10 | The real `OBS.tsv` files. |

No copy of these was found, so they are absent:

- en_tq v91 (the Questions tab of a Bible project)
- es-419_twl v18, es-419_tw v37 and es-419_ta v4 (the primary set of the Bible sample)
- es-419_obs v2, es-419_obs-tn v2 and es-419_obs-twl v2 (the primary set of the OBS sample)
- en_obs v9 stories 2-50, and the picture pack uW/obs_images_360
- the lexicons en_ugl and en_uhl, and hbo_uhb (no captured screen reads them, but the sync
  toasts count hbo_uhb v3.0.0 as a missing version)
- helps for Jonah, the Bible sample's second book
- en_ta `translate/figs-inclusive`, which es-419_tn v66 names; en_ta v86 and v88 do not have it

### Scenarios

| Scenario | State |
|---|---|
| `sample` | The conformance samples exactly as tC4 ships them. The Bible sample pins an **es-419 primary set** with an English fallback. Its es-419 TWL, tW and tA are absent, so the Translation Words tool reads "Needs downloading". |
| `en-pins` | Both samples are pinned to the installed English suite on both rungs. This is how tC4 pins a project it creates. For the Bible sample, this is tC4's own J4 journey setup (`e2e/helpers/rig.ts` `writeProjectPins`, plus `restateRecordAsEnglish` for both decision files). For the OBS sample, the primary set becomes a copy of its English fallback set. The change is applied before the project is first opened. |
| `shared` | The scenario `sample`, with both samples shared on Door43 as a first share leaves them: each has the remote `origin` at `https://git.door43.org/facilitator-zq/<project>.git` (in the mock's `.remotes.json`). The owner is the fake Door43 user. Door43 holds no copy of either sample until a screen's steps push one. |

Each screen starts from a reload of Home. The per-client settings document is reset to its
seeded form (install records only) first, so no screen depends on a place that an earlier screen
remembered. The pointer rests on an empty part of the title bar for every shot.

## The screenshots

tC4 has no router: the URL is always `/clients/uw-tc4/`, and a screen is the app state that the
steps reach. "View" is tC4's `s.view` (`read` is Understand, `draft` is Translate, `publish` is
Community Checking), plus the dialog or panel over it. Every path starts at Home. The interface
is in English, except for the screens whose name starts with `ar-`, where Arabic is saved as the
app language before the launch. "Status" is
`complete` when the screen shows what a full install shows. It is `incomplete` when part of the
screen shows a designed absence state because data is missing; the next section says why, and
`manifest.json` has the same text (`incomplete`) and the reads that failed (`absentReads`).

| Screenshot | Scenario | View | Steps after Home | Shows | Status |
|---|---|---|---|---|---|
| `home` | sample | home | Launch tC4 | Home: "Your projects" with the OBS sample (story tiles, 1% drafted) and the Bible sample (Jonah and Titus tiles, "+ Add a book") | complete |
| `understand-tit1` | sample | read | Bible sample card: Titus tile | Understand, Titus 1, Section view: book rail, ULT source units with the comment box, helps panel on Notes (es-419 Translation Notes of the primary set) | complete |
| `translate-tit1` | sample | draft | Bible sample card: Titus tile → Mode switch: Translate | Translate, Titus 1, Section mode: ULT beside the es-419 draft, each "Draft section" a pill in the accent colour at the end of its row, helps panel with the Comments tab | complete |
| `check-picker` | sample | check | Bible sample card: Titus tile → Mode switch: Check | Check tool picker for Titus: Translation Words "Needs downloading", Translation Notes ready (1 of 112), Align, Community Checking | incomplete |
| `check-tn-session` | sample | check | Bible sample card: Titus tile → Mode switch: Check → Translation Notes card: Continue | Translation Notes session on the es-419 notes, resumed at item 2 ("knowledge", 1:1) after the decided item 1: check rail (filters, sort, list), the quote, WHAT TO CHECK with Show more, the compare card (Greek, ULT highlight, the es-419 draft), the marking row ("← Previous", the triage buttons, "Next →") below the fold; the header shows only "Item 2 of 112" | incomplete |
| `align-workspace` | sample | check | Bible sample card: Titus tile → Mode switch: Check → Align card | Align, Titus 1:1, Gateway · ULT mode: the align rail (46 verses, 5 drafted), the ULT verse, Suggest links (off), the word bank of the es-419 draft in verse order, with an occurrence number on each repeated word (6 of 27 placed), and the ULT word cards with their Greek and their target words in verse order, the sample's six links placed | complete |
| `align-original` | sample | check | Bible sample card: Titus tile → Mode switch: Check → Align card → Align mode: Original · Greek | Align, Titus 1:1, Original · Greek mode: the Greek verse and one word card per Greek word, the six placed links, an occurrence number on each repeated word (Θεοῦ¹, Dios¹), the word bank in verse order, and the top of the marking row ("← Previous", Mark valid, "Next →") at the bottom edge | complete |
| `align-workspace-row` | sample | check | Bible sample card: Titus tile → Mode switch: Check → Align card → Scroll the marking row into view | align-workspace scrolled to the marking row: the word cards, then "← Previous", Mark valid and "Next →" under the card | complete |
| `ar-align-workspace-row` | sample | check | App language: العربية (saved before the launch) → Bible sample card: Titus tile → Mode switch: مراجعة (Check) → Align card → Scroll the marking row into view | align-workspace with the interface in Arabic, scrolled to the marking row: the chrome and the marking row run right to left ("→ السابق", Mark valid, "التالي ←", with the arrows turned); the ULT, the Greek and the draft stay left to right | complete |
| `align-workspace-620` | sample | check | Window 620 px wide → Bible sample card: Titus tile → Mode switch: Check → Align card → Scroll the marking row into view | align-workspace in a window 620 px wide, scrolled to the marking row, which wraps onto two lines ("Next →" alone on the second); at this width the project button in the header lies over the mode switch | complete |
| `ar-align-workspace-620` | sample | check | Window 620 px wide → App language: العربية (saved before the launch) → Bible sample card: Titus tile → Mode switch: مراجعة (Check) → Align card → Scroll the marking row into view | ar-align-workspace-row in a window 620 px wide: the marking row wraps onto two lines, right to left ("التالي ←" alone on the second) | complete |
| `project-settings` | sample | home + settings | Bible sample card: Settings | Project settings dialog: read-only name/language/code, text direction chips, checking language card (English package offered), script font | complete |
| `add-project` | sample | home + addProject | + Add a project | Add a project chooser with four cards: New Bible, New Open Bible Stories, Import a project, Open a project from Door43 | complete |
| `new-bible` | sample | home + newProject | + Add a project → New Bible | New Bible dialog, empty: name, language name, code, text direction, checking language (English, Recommended), script font, Versification disclosure | complete |
| `import` | sample | home + import | + Add a project → Import a project | Import a project, step 1 of 3: tC3 project, USFM files, Scripture Burrito | complete |
| `share-internet-consent` | sample | home + netAsk | Bible sample card: Share on Door43 | "Turn on the internet?" consent dialog that every internet step asks first while the internet is off | complete |
| `share-signin` | sample | home + signIn | Bible sample card: Share on Door43 → Turn on internet | Sign in to Door43 dialog (signs in to git.door43.org), empty fields, and the notice that the Door43 username goes into the project, stays in it, and goes into each Scripture Burrito export | complete |
| `share-dialog` | sample | home + share | Bible sample card: Share on Door43 → Turn on internet → Sign in as facilitator-zq | Share on Door43, step "where it goes": your account (selected), an organization marked Recommended, one you cannot create in | incomplete |
| `share-dialog-check` | sample | home + share | Bible sample card: Share on Door43 → Turn on internet → Sign in as facilitator-zq → Next | Share on Door43, step "Check what will be shared": target, repository name, books shared | incomplete |
| `account-menu` | sample | home + account menu (popover) | Account button (top right) | Account menu: Sign in to Door43, Use the internet (off), Help and guides, App language, Report a problem, About translationCore | complete |
| `app-language` | sample | home + app language | Account button → App language | App language dialog with English selected | complete |
| `help` | sample | home + help and guides (full-window panel) | Account button → Help and guides | Help and guides: the full-window panel with the help pages of the build (tc-website tc-help/ at the commit tC4 pins) open on their index | complete |
| `about` | sample | home + about | Account button → About translationCore | About translationCore: version and commit of the build, copyright line, license name and Read the license | complete |
| `feedback` | sample | home + feedback | Account button → Report a problem | Report a problem dialog, empty message | complete |
| `new-obs` | sample | home + newObs | + Add a project → New Open Bible Stories | New Open Bible Stories dialog, empty | complete |
| `add-book` | sample | home + addBook | Bible sample card: + Add a book | Add a book to the Bible sample: Start a blank book, or Add books from USFM files | complete |
| `source-texts` | sample | home + sources | Bible sample card: Settings → Manage source texts | Get source texts: Pinned resources (Check for updates) and the gateway-language packages to choose from. Opening it opens the project, so Understand (Jonah) is behind it | complete |
| `guided-fix` | sample | check + fix | Bible sample card: Titus tile → Mode switch: Check → Translation Words card: Fix this | Guided fix for the Translation Words tool: what is missing (the es-419 TWL/tW pins) and the ways to get it | complete |
| `obs-understand` | sample | read | OBS sample card: story tile "1 · La Creación" | OBS Understand, story 1: story rail with the frame grid, one unit per frame with the comment box, helps panel | incomplete |
| `obs-translate` | sample | draft | OBS sample card: story tile "1 · La Creación" → Mode switch: Translate | OBS Translate, story 1: the gateway-story and picture notices, the title and frames 1-2 drafted in Spanish, "Draft frame N" pills | incomplete |
| `obs-check-picker` | sample | check | OBS sample card: story tile "1 · La Creación" → Mode switch: Check | OBS Check tool picker: Translation Words and Translation Notes "Needs downloading", Community Checking (no Align for OBS) | incomplete |
| `open-door43` | sample | home + openDoor43 | + Add a project → Open a project from Door43 | Open a project from Door43 dialog, empty: the note "The project opens from git.door43.org. Only public repositories can be opened.", Door43 repository, Project name, the note that tC4 checks every file before it adds the project, Cancel / Open project (disabled) | complete |
| `open-door43-internet-consent` | sample | home + netAsk | + Add a project → Open a project from Door43 → Repository: facilitator-zq/es-419_tit_equipo → Open project | "Turn on the internet?" asked for an open from Door43 (kind `open`): get a full copy of the project and check its files; nothing from your projects is sent | complete |
| `open-door43-refused` | sample | home + openDoor43 | + Add a project → Open a project from Door43 → Repository: facilitator-zq/es-419_tit_equipo → Open project → Turn on internet | Open a project from Door43 dialog with the refusal callout "This project cannot be opened." for a clone that failed (code sync.clone-failed), "No project was added." and Ask for help. The clone fails because Door43 has no repository of that name | complete |
| `en-understand-tit1` | en-pins | read | Bible sample card: Titus tile | Understand, Titus 1, Section view, helps panel on Notes with the English Translation Notes v91 (intro note, abstract-nouns note) | complete |
| `en-understand-tw-article` | en-pins | read | Bible sample card: Titus tile → Helps tab: Words → Card "of God": Read the article | Understand with the Words tab (Key word cards) and the Translation Words article pane open on "God" (kt/god.md) | complete |
| `en-understand-questions` | en-pins | read | Bible sample card: Titus tile → Helps tab: Questions | Understand with the Questions tab selected | incomplete |
| `en-translate-tit1` | en-pins | draft | Bible sample card: Titus tile → Mode switch: Translate | Translate, Titus 1, Section mode, English helps, each "Draft section" a pill in the accent colour at the end of its row | complete |
| `en-translate-section-editor` | en-pins | draft | Bible sample card: Titus tile → Mode switch: Translate → Draft section 1–3 | Section editor open on Titus 1:1-3 (Type mode): one "N body" line per verse, Save section / Cancel; while it is open, the Understand and Check tabs, the book rail and the other "Draft section" pills are disabled | complete |
| `en-translate-verse-editor` | en-pins | draft | Bible sample card: Titus tile → Mode switch: Translate → Unit switch: Verse → Verse 2 (drafted text) → Scroll the editor to the middle of the pane | Verse mode with the verse editor open on Titus 1:2 ("Editing below" marker, "DRAFTING 2" card with Save verse / Cancel); while it is open, the Understand and Check tabs and the book rail are disabled | complete |
| `en-check-picker` | en-pins | check | Bible sample card: Titus tile → Mode switch: Check | Check tool picker with every tool ready: Translation Words (2 of 182), Translation Notes (0 of 157), Align (0 of 5 verses), Community Checking | complete |
| `en-check-tw-session` | en-pins | check | Bible sample card: Titus tile → Mode switch: Check → Translation Words card: Start checking | Translation Words session (sorted By word), item "Paul" 1:1: rail, "Learn more: Paul, Saul", compare card (Greek, ULT highlight, the es-419 draft), the marking row ("← Previous", Mark valid, Invalid, To do, "Next →"), Bookmark / Add a comment; the header shows only "Item 1 of 182" | complete |
| `en-check-tw-article` | en-pins | check + academy drawer | Bible sample card: Titus tile → Mode switch: Check → Translation Words card: Start checking → Rail item "of God" (1:1) → Learn more: God | Translation Words session on the decided item "of God" with the article drawer open on "God" (kt/god.md) | complete |
| `en-check-tn-session` | en-pins | check | Bible sample card: Titus tile → Mode switch: Check → Translation Notes card: Start checking | Translation Notes session on the English notes v91, item 1 (figs-abstractnouns, 1:1): the Greek verse, the quote highlighted in the ULT, the es-419 draft | complete |
| `en-check-ta-drawer` | en-pins | check + academy drawer | Bible sample card: Titus tile → Mode switch: Check → Translation Notes card: Start checking → Learn more: Abstract Nouns | Translation Academy drawer on "Abstract Nouns" (translate/figs-abstractnouns) over the Translation Notes session | complete |
| `en-community-checking` | en-pins | publish | Bible sample card: Titus tile → Mode switch: Check → Community Checking card: Open | Community Checking: the typeset A4 preview of Titus (drop-cap chapter, "[ verse not yet drafted ]" runs) and the side panel with Export and Page setup | complete |
| `en-obs-understand` | en-pins | read | OBS sample card: story tile "1 · La Creación" | OBS Understand, story 1: the English gateway frames with a comment box each, helps panel with the English OBS notes (en_obs-tn v13) | incomplete |
| `en-obs-translate` | en-pins | draft | OBS sample card: story tile "1 · La Creación" → Mode switch: Translate | OBS Translate, story 1: the English gateway story beside the Spanish title and frames, English OBS helps | incomplete |
| `en-obs-check-tn-session` | en-pins | check | OBS sample card: story tile "1 · La Creación" → Mode switch: Check → Translation Notes card: Start checking | OBS Translation Notes session, story 1 (sorted By frame), item "The Creation" 1:0: the title note, the English gateway title highlighted, the Spanish title, the marking row with "← Previous" and "Next →" | complete |
| `ar-check-tw-session` | en-pins | check | App language: العربية (saved before the launch) → Bible sample card: Titus tile → Mode switch: مراجعة (Check) → Translation Words card: Start checking | en-check-tw-session with the interface in Arabic: the whole chrome right to left, and the marking row "→ السابق", the triage buttons, "التالي ←", with the arrows turned for right to left | complete |
| `en-check-tw-session-620` | en-pins | check | Window 620 px wide → Bible sample card: Titus tile → Mode switch: Check → Translation Words card: Start checking → Scroll the marking row into view | en-check-tw-session in a window 620 px wide, scrolled to the marking row, which wraps onto three lines; at this width the project button in the header lies over the mode switch | complete |
| `ar-check-tw-session-620` | en-pins | check | Window 620 px wide → App language: العربية (saved before the launch) → Bible sample card: Titus tile → Mode switch: مراجعة (Check) → Translation Words card: Start checking → Scroll the marking row into view | ar-check-tw-session in a window 620 px wide, scrolled to the marking row, which wraps onto three lines, right to left | complete |
| `en-check-tn-session-row` | en-pins | check | Bible sample card: Titus tile → Mode switch: Check → Translation Notes card: Start checking → Scroll the marking row into view | en-check-tn-session scrolled to the marking row: the end of WHAT TO CHECK, "Learn more: Abstract Nouns", the compare card, then "← Previous", Mark valid, Invalid, To do, "Next →", and Bookmark / Add a comment | complete |
| `ar-check-tn-session-row` | en-pins | check | App language: العربية (saved before the launch) → Bible sample card: Titus tile → Mode switch: مراجعة (Check) → Translation Notes card: Start checking → Scroll the marking row into view | en-check-tn-session-row with the interface in Arabic: the chrome and the marking row run right to left ("→ السابق", the triage buttons, "التالي ←", with the arrows turned) | complete |
| `en-check-tn-session-620` | en-pins | check | Window 620 px wide → Bible sample card: Titus tile → Mode switch: Check → Translation Notes card: Start checking → Scroll the marking row into view | en-check-tn-session in a window 620 px wide, scrolled to the marking row, which wraps onto three lines; at this width the project button in the header lies over the mode switch | complete |
| `ar-check-tn-session-620` | en-pins | check | Window 620 px wide → App language: العربية (saved before the launch) → Bible sample card: Titus tile → Mode switch: مراجعة (Check) → Translation Notes card: Start checking → Scroll the marking row into view | ar-check-tn-session-row in a window 620 px wide: the marking row wraps onto three lines, right to left | complete |
| `shared-home` | shared | home | Launch tC4 | Home with both samples shared on Door43: "On Door43" beside each name; the Bible card reads "Sync with Door43", the OBS card "Upload changes" (tC4 4.0 syncs Bible projects only) | complete |
| `sync-internet-consent` | shared | home + netAsk | Bible sample card: Sync with Door43 | "Turn on the internet?" asked for a sync (kind `sync`): get the changes of your teammates and send the changes of this project. Behind the scrim, the OBS card reads "Upload changes" | complete |
| `sync-failed` | shared | home | Door43 main: the Bible sample as it is now (the push of its first share) → Door43: the connection drops at the next clone of the Bible sample's repository → Bible sample card: Sync with Door43 → Turn on internet → Sign in as facilitator-zq | Home with the sync result toast of a sync that did not complete: red dot, "The sync did not complete." and the sentence of code sync.clone-failed, the dismiss button. Door43 has main, so the sync starts, and then the clone of main fails because the connection to git.door43.org drops | complete |
| `sync-ok` | shared | home | Bible sample card: Titus tile → Home → Door43 main: the Bible sample as it is now (the push of its first share) → Bible sample card: Sync with Door43 → Turn on internet → Sign in as facilitator-zq | Home with the toast of a sync that completed with no change from teammates: kindle dot, "Synced with Door43 · 0 changes from teammates · 0 conflicts kept the newest version · 0 verse conflicts to review", the note "This project now uses 5 resource versions that are not on this computer.", Download and the dismiss button | incomplete |
| `sync-ok-teammates` | shared | home | Bible sample card: Titus tile → Home → Door43 main: the Bible sample as it is now (the push of its first share) → Door43 main: two teammates change Titus 1:1 from the same version → Bible sample card: Sync with Door43 → Turn on internet → Sign in as facilitator-zq | Home with the toast of a sync that brought two teammates' changes to Titus 1:1: kindle dot, "Synced with Door43 · 2 changes from teammates · 0 conflicts kept the newest version · 1 verse conflict to review", "Changes from: Teammate 1 · Teammate 2", the note on the 5 missing resource versions, Review, Download and the dismiss button; the Bible card shows the chip "1 conflict · Review" and the Titus tile "1 conflict" | incomplete |

## Incomplete screens and why

Every incomplete screen still shows tC4's real chrome and layout. The missing part is a state
that tC4 designed for absent data, so that state is part of the baseline too.

- **es-419 helps absent (scenario `sample`).** `check-picker`: the Translation Words card reads
  "Needs downloading" (`en-check-picker` is the ready version; `guided-fix` is the dialog it
  opens). `check-tn-session`: the link under WHAT TO CHECK reads "Learn more: Translation
  Academy" instead of the article title, because es-419_ta is absent (`en-check-tn-session` shows
  the titled link).
- **es-419 OBS set absent (scenario `sample`).** `obs-understand` and `obs-translate`: every
  gateway frame reads "Gateway text is unavailable.", the helps say "The pinned resource is not on
  this computer yet", and Translate shows the "pinned gateway story … is not on this machine / Get
  source" notice. `obs-check-picker`: both helps tools read "Needs downloading". The `en-obs-*`
  screens are the English versions.
- **No en_tq: `en-understand-questions`.** The tab shows "The pinned resource is not on this
  computer yet. Get it from Source texts."
- **No OBS pictures: `en-obs-understand` and `en-obs-translate`** (and the two `obs-*` screens).
  No frame has its picture. Translate shows tC4's notice that lists the packs it read, and the
  pack "lists 0 image files": that is how pankosmia-web answers for a folder that is not there.
- **Door43 is faked: `share-dialog` and `share-dialog-check`.** The account `facilitator-zq` and
  the organizations `Equipo-Ejemplo` and `es-419_gl` are invented by `fake-door43.mjs`, and
  nothing is created or pushed.
- **Missing resource versions: `sync-ok` and `sync-ok-teammates`.** The toast's note counts
  the 5 versions that the Bible sample pins and the mock lacks: es-419_twl v18, es-419_tw v37,
  es-419_ta v4, en_tq v91 and hbo_uhb v3.0.0. `syncToast.missingVersions` lists them, by tC4's
  own rule. With every pinned version on the computer, the toast shows no such note and no
  Download, and the dot of `sync-ok` is green. tC4's desktop package ships en_tq v91 and
  hbo_uhb v3.0.0 (`scripts/package-desktop.zsh`), so a packaged install that also holds the
  mock's resources counts 3.

These are also known differences from a real install. None of them makes a screen incomplete:

- Substitute content (see the data table). The ULT and UST read "v91" but the text is v89. Most
  tW and tA articles are v88 under v91 labels. The v89 and v91 Titus texts were not compared.
- The UGNT file is rebuilt from translationCore 3's JSON, so its whitespace and book headers are
  not the export's. tC4 shows neither.
- `feedback` shows the platform line of the attachment, "Linux x86_64", from the browser. Another
  machine shows its own platform.
- `project-settings` shows an empty Language name. This is tC4's behavior for this sample, whose
  `settings.json` has no `languageName`. It is not a mock gap.
- `source-texts` opens the project first, so Understand (Jonah, the first book) is behind the
  dialog, and its helps panel reads "The installed resource has nothing for this book."

## Where the rebuild differs on purpose

The project owner ruled that the rebuild differs from tC4 4.0 in some places. `docs/PLAN.md`
lists these rulings under "Where the rebuild differs from tC4 4.0 on purpose". On the screens
below, do not copy tC4's text into the rebuild. The rest of each screen is still the reference.
The entry of each screen in `manifest.json` has the same note in `rebuildDiffers`.

| Screen | Ruling | What the rebuild shows |
|---|---|---|
| `open-door43` | Q3 | The note has no sentence that only public repositories can be opened. A new key replaces `openDoor43.where` in all 10 languages. A private repository opens after sign-in. |
| `open-door43-refused` | Q3 | tC4's sentence for this refusal, `openDoor43.refused.other`, says "Make sure that the repository is public". The rebuild opens private repositories, so its sentence must not say this. The delta document replaces only `openDoor43.where`, so the new text of this sentence is not decided yet. When Door43 answers 401 or 404 and nobody is signed in, the rebuild shows the sign-in step instead of this refusal. |
| `shared-home` | Q2 | The OBS card reads "Sync with Door43", like the Bible card. The rebuild syncs OBS projects too. |
| `sync-internet-consent` | Q2 | Behind the scrim, the OBS card reads "Sync with Door43". |
| `sync-failed` | Q2 | The OBS card reads "Sync with Door43". |
| `sync-ok` | Q2 | The OBS card reads "Sync with Door43". |
| `sync-ok-teammates` | Q2 | The OBS card reads "Sync with Door43". |

The other rulings change no captured screen. For example, the Align screens show Titus 1:1,
which has a saved alignment, so tC4 already shows its word bank in verse order there (Q15).
On `sync-ok-teammates` the one conflict is a verse conflict, so the rebuild shows Review there
too (Q5).

## Changes since d494a9d

16 of the 42 screens changed from `d494a9d` to `d310bf3`. The other 26 are byte-identical. The
mock, its data and the capture steps are the same for both commits, so tC4 made every change.
"Pixels" counts the pixels that differ. `history/d494a9d/` has the old image of each screen.

| Screen | Pixels | What changed | tC4 change |
|---|---:|---|---|
| `add-project` | 198,667 | A fourth card, "Open a project from Door43". The dialog is taller. | #692 |
| `share-signin` | 165,128 | A new paragraph: the Door43 username goes into the project, stays in it, and goes into each Scripture Burrito export. | #691 |
| `align-workspace` | 185,303 | The word bank is in verse order. Each repeated word shows its occurrence number, in the bank and on the cards. The words on each card are in verse order. The arrows left the header, which now shows only "Item 1 of 5", so the page moved up. | #744, #750, #752, #745 |
| `align-original` | 187,214 | The same as `align-workspace`. The marking row, "← Previous", Mark valid and "Next →", now shows at the bottom edge. | #744, #750, #752, #745 |
| `check-tn-session` | 116,364 | The arrows left the header, so the page moved up. The marking row is below the fold. | #745 |
| `en-check-tn-session` | 132,887 | The same as `check-tn-session`. | #745 |
| `en-check-ta-drawer` | 94,245 | The same, behind the drawer. | #745 |
| `en-check-tw-article` | 80,583 | The arrows left the header, so the page moved up. Behind the drawer, the marking row starts with "← Previous". | #745 |
| `en-check-tw-session` | 116,053 | The arrows left the header. The marking row now reads "← Previous", Mark valid, Invalid, To do, "Next →". | #745 |
| `en-obs-check-tn-session` | 91,213 | The same as `en-check-tw-session`, for an OBS story. | #745 |
| `translate-tit1` | 55,784 | "Draft section" is a pill in the accent colour at the end of its row, not a plain text button. | #745 |
| `en-translate-tit1` | 55,784 | The same as `translate-tit1`. | #745 |
| `en-translate-section-editor` | 4,995 | While the editor is open, the Understand and Check tabs, the book rail and the other "Draft section" pills are disabled. | #698 |
| `en-translate-verse-editor` | 3,237 | While the editor is open, the Understand and Check tabs and the book rail are disabled. | #698 |
| `about` | 460 | The build commit reads `d310bf3`. This is not a design change. | none |
| `feedback` | 313 | The build commit in the attachment line reads `d310bf3`. This is not a design change. | none |

Two changes do not show on these screens:

- `home`: tC4 now shows "— in progress" until it knows every book's progress (#693). The shot
  waits until the app stops reading, so the count is known by then.
- `help`: topic pages now swap in place without a white flash (#738). The shot shows the index
  page only.

## Screens added for d310bf3

`docs/TC4-DELTA-2026-10-16.md` §3.14 item 3 asks for new screens. These 19 were added:

| Screen | What it shows | tC4 change |
|---|---|---|
| `open-door43` | The Open a project from Door43 dialog, empty. | #692 |
| `open-door43-internet-consent` | The internet question of an open from Door43 (kind `open`). | #692 |
| `open-door43-refused` | The dialog's refusal callout, for a clone that failed. | #692 |
| `ar-check-tw-session` | The Translation Words session with the interface in Arabic. The marking row runs right to left, and its arrows turn. | #745, #739 |
| `en-check-tw-session-620` | The same session in a window 620 px wide. The marking row wraps. | #745 |
| `ar-check-tw-session-620` | The Arabic session in a window 620 px wide. The marking row wraps, right to left. | #745, #739 |
| `en-check-tn-session-row` | The Translation Notes session, scrolled to its marking row, which is below the fold at 800 px. | #745 |
| `ar-check-tn-session-row` | The same in Arabic. | #745, #739 |
| `en-check-tn-session-620` | The Translation Notes session in a window 620 px wide, at its marking row. The row wraps. | #745 |
| `ar-check-tn-session-620` | The same in Arabic. | #745, #739 |
| `align-workspace-row` | Align, scrolled to its marking row, which is below the fold at 800 px. | #745 |
| `ar-align-workspace-row` | The same in Arabic. | #745, #739 |
| `align-workspace-620` | Align in a window 620 px wide, at its marking row. The row wraps. | #745 |
| `ar-align-workspace-620` | The same in Arabic. | #745, #739 |
| `shared-home` | Home with shared projects: the Bible card reads "Sync with Door43", the OBS card "Upload changes". | #690 |
| `sync-internet-consent` | The internet question of a sync (kind `sync`). | #690 |
| `sync-failed` | The sync result toast of a sync that did not complete. | #690, #691 |
| `sync-ok` | The toast of a sync that completed with no change from teammates: the count line, the note on missing resource versions and Download. | #690, #691 |
| `sync-ok-teammates` | The toast of a sync that brought teammates' changes and a verse conflict: the count line, "Changes from", the note on missing resource versions, Review and Download. | #690, #691 |

Already in the 42 screens: Add a project with four cards (`add-project`) and the sign-in step
with the name notice (`share-signin`).

The marking row of Translation Words, Translation Notes and Align is now captured in English
and in Arabic, at 1280 px and at 620 px. Each such screen has `markingRow` in `manifest.json`.
The Translation Notes and Align rows are below the fold at 800 px, so their screens scroll the
row into view. tC4's own images of these rows, cropped to the row, are in its
`docs/evidence/navigation-pills-2026-10-15/` at `d310bf3`.

Every part of the sync result toast now has a reference image. tC4 has no committed image of
this toast; its `e2e/sync-button.spec.ts` writes screenshots only as test output on its rig.
`sync-ok` and `sync-ok-teammates` follow that spec's two syncs. The project is opened once
before it is pushed, because tC4 writes the sample's journal when it first opens it. Without
that journal on Door43, tC4 stops the sync with `sync.verify-failed`. Not captured: the
conflict review that Review opens, and an open from Door43 that succeeds (J17).

## Rendering determinism

`capture.mjs` launches Chromium with `--force-color-profile=srgb --disable-partial-raster
--disable-gpu-rasterization --disable-lcd-text --font-render-hinting=none`. Without these flags, a
few rounded edges differed by up to 8/255 between runs. With them, consecutive full runs give
identical PNG bytes.

The run also controls the following:

- Locale `en-US`, time zone `UTC` and the light color scheme.
- Each shot waits until no `/api` request has been in flight for 800 ms and the fonts are loaded,
  and is taken with `animations: 'disabled'`. The help shot also waits for the frame's images.
- `absentReads` in the manifest are sorted, because concurrent reads finish in a different
  order from run to run.
- The browser can reach no origin except the mock server and the faked Door43. Any other request
  is refused and listed in `refusedExternalRequests` in the manifest. The list is empty.

## Using the baseline for the Rust rebuild

`.state/<scenario>/repos` uses the Pankosmia working-directory layout. Local projects are under
`_local_/_local_/` and resources under `_local_/_sideloaded_/`. The install records are in
`client_settings/uw-tc4.json`. A Rust build can therefore open the same projects and resources and
render the same screens at 1280x800. For each screen, follow the `steps` in `manifest.json` from
a fresh state of the same scenario (`npm run state -- --scenario <name>`). A step that starts
with "Door43" changes Door43, not the app: do it on the rebuild's fake Door43. A screen with a
`viewport` in the manifest uses that window size, and a screen with a `locale` uses that app
language. A screen with `rebuildDiffers` matches tC4 except for what that note says ("Where the
rebuild differs on purpose"). The two renderers differ, so a comparison needs a tolerance.
