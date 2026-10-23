# Plan for design unit B9a: settings, source texts and downloads

Worktree `.claude/worktrees/lane-b9a`, branch `lane-b9a`, cut from `claude/focused-ride-5uzj7p`
at `d250a89`. Row (docs/APP-DESIGN.md §10): M7 read with `viewFromHome` (AD14); M8 steps 1 and
2; download jobs over A11's streaming (`install_dcs_watched` with a `DownloadWatch` over the
job's counters and cancel token); the `source-texts` Urdu mask (#701). Done when:
settings-contract (read part); cancel leaves no partial install. Parity: `project-settings`,
`source-texts`. No Q-n waits. Issues: #579 (autonyms), #606 (Aquifer attribution), #120
(downloads from production), #641 (one server read).

Only what a translator reaches (owner, 2026-10-19). tC4 opens Settings only from a Home card
(#505), and every way into Get source texts opens the project first (Settings' "Manage source
texts", Check's "Get resources", the story notices). So:

- `settings/viewFromHome` only. `settings/view` (Settings inside an open project) has no route
  in tC4 4.0; it is not built, and §3.9 says so.
- No Aquifer asset gets a `Download` job: M8 lists one Door43 organization's packages, and
  nothing in the app installs an Aquifer asset. The Aquifer streaming and #606's Aquifer line
  wait for the unit that first installs one; §4.6 and TC4-ISSUES say so.

Steps, each a commit once its crate's tests and clippy pass:

1. **Server.** tc-api `settings/viewFromHome {project}` -> `SettingsView` (language name,
   direction, font, the checking language card: current from the pins by autonym, the
   installed packages that cover the project's books); `resources/sources {at}` (step 1: the
   18 packages with tC4's `seen` versions, this session's installed count, the project's
   current gateway, the checkable ones, whether it has language sets), `resources/catalog {at,
   gateway, book}` [sources] (step 2: tC4's `packageRows` over Door43's catalog for the owner),
   `resources/download {request, at, gateway, book, repos}` [sources] (job `Download`).
   tc-app: settings from the derived files through a new `ProjectStore::settings_files` (no
   open, no fold; memory renders them); a second Door43 client for resources on the
   production server (`AppConfig::resources`, #120; tests point it at the fake); the download
   job takes the project's pin of a repository when this computer lacks it, else what is
   installed, else the latest release, installs with `install_dcs_watched` on a
   `DownloadWatch` over the job's progress and cancel token, clears the session's book cache,
   and counts the package for step 1.
   Tests (tc-app, fake Door43): the settings-contract read part (direction, font, language
   name, checking language from the pins by autonym, an obsolete `checkingLanguage` ignored),
   a held settings file states its error; catalog rows for a book; download installs; a cancel
   leaves no install and no partial file; downloads use the resources server, never the
   account server; tc4: a development build downloads from production.
2. **UI.** tc-ui Project settings (read; direction and font chips change the form; Save is
   C6's) from Home's Settings; Get source texts steps 1 and 2 over the open project from
   "Manage source texts" (B3's open), with "Check for updates" and "Use for checking" named
   and empty (C8). A Cancel while a download runs is a declared difference (tC4 has none).
3. **Parity.** Capture `project-settings` and `source-texts` on the store; the Urdu row mask
   cites #701.
4. **Docs.** APP-DESIGN B9a row refinements, §3.9, §4.6; TC4-ISSUES rows.
