# Gap: release-pipeline-and-app-identity

## Summary
No branch has a release pipeline. claude/focused-ride-5uzj7p, resume-build-lane, s2-proof-executor and storage each hold only .github/workflows/ci.yml, and that file uses only ubuntu-latest runners. There is no production Electronite shell either: spikes/electronite is marked "never shipped". The lane does have most of the payload: the UI build (crates/tc-ui/build-app.sh), `tc4 serve --bundle` and the bundle_store example. It has no staging step, no installer, no signing, no notices file, no OBS picture download and no smoke tests. G1 is the only unit that owns any of this. Its design row (APP-DESIGN.md:3425) names only a Windows runner and leaves out installers, signing and notices, even though TC4-ISSUES.md gives G1 the issues #44, #456, #554 and #606. G1 also waits for D2 (exports).

Several owner decisions block this work:
- Which platforms count for "done".
- The app's identity: the product name, Windows AppId and bundle id, the version number, and the app id, name and download page that TC4-REQUEST-MOVED-NOTICE.md:34-35 says the first release must fix.
- Whether first launch looks for tC4 4.0 projects in ~/pankosmia/tc4-projects.
- Who adds an Azure OIDC federated credential for the rebuild's repository. The existing one is bound to unfoldingWord/translationCore4, and the rebuild's remote is github.com/birchamp/tc-rst.
- Which runners the workflow may use.

One problem a translator can reach today: the help pages the rebuild ships byte for byte say projects live in ~/pankosmia/tc4-projects and tell the translator to back up that folder. The rebuild keeps projects in ~/translationCore4/projects.

Proposed split: G1a shell, G1b staging and Linux zip with notices and the OBS fetch, G1c Windows installer and signing, G1d macOS (only if in scope), G1e installed smoke and offline run, G1f P8 Graphite probe, and G1g identity and migration. Each is done when a CI job that can fail passes.

# Release pipeline and app identity: what exists, what is missing, and who owns it

Snapshot paths used below:
- **SNAP** is resume-build-lane at f9067bc: `/tmp/claude-1000/-home-bot-tc-rst/ecfd5b65-e7c2-41f7-94cb-0393c31b1f98/scratchpad/snap/resume-build-lane`
- **TC4** is tC4 main at b86270f: `/tmp/claude-1000/-home-bot-tc-rst/ecfd5b65-e7c2-41f7-94cb-0393c31b1f98/scratchpad/ext/tc4-tip`

tC4's packaging files are the same at rc.2 (a5ce0307) and at main: `git diff --stat a5ce0307 b86270f` over PACKAGING.md, package-desktop.yml/.zsh, tc4.iss, desktop-*.cjs, smoke-installed.zsh and sign-windows.ps1 printed nothing.

Facts checked once and used throughout:
- **Workflows on every branch.** `git ls-tree -r --name-only <branch> .github` lists only `.github/workflows/ci.yml` on claude/focused-ride-5uzj7p, resume-build-lane, s2-proof-executor and storage.
- **What the rebuild's CI runs.** `ci.yml` has three jobs, all on `ubuntu-latest` (SNAP ci.yml:11-12, 47-48, 65-66). Its browser tests skip in CI (ci.yml:55-64).
- **The rebuild's remote.** `git remote -v` gives origin = `https://github.com/birchamp/tc-rst`.
- **The workspace version** is `0.1.0` (SNAP Cargo.toml:9), with the license GPL-2.0-or-later (:11).
- **G1's status.** It is "to do", waiting for D2 (SNAP .audit/STATUS.md:74). G2 is "to do", waiting for P (:75).

## (a) Release deliverables

| Deliverable | tC4 source | Rebuild status | Owning unit | Runner or OS | Credential or owner decision | Unblocks |
|---|---|---|---|---|---|---|
| Release workflow (the umbrella) | TC4 .github/workflows/package-desktop.yml:1-588: a preflight job (:70-84), three build jobs (:86, :161, :252) and three smoke jobs (:361, :466, :536) | **absent** (see the workflow facts above) | none | ubuntu-24.04, windows-2025 and macos-15, the runners tC4 uses (:72, :92, :163, :254) | Which runners are allowed (decision b7) | Every row below |
| Windows x64 installer | TC4 scripts/tc4.iss:12-51: AppId `{24B10730-6260-4EB7-A2EA-F3F19C90F242}` (:13), per-user install with no elevation (:17-19), AppUserModelID `org.unfoldingword.translationcore4` (:50-51). Built at package-desktop.zsh:1084-1095. Documented in PACKAGING.md:488-544 and :664-666 (uninstall keeps projects) | **absent** | G1, through TC4-ISSUES.md:396 (#44). The G1 design row (APP-DESIGN.md:3425) does not list an installer | windows-2025 has Inno Setup 6.7.1 (PACKAGING.md:467-468). The Windows VM has no Inno Setup (.audit/tools/winvm/setup.ps1:1-50) | Identity: same AppId and name as tC4 or not (b2); the version number (b3) | PERFORMANCE.md:114 (first launch); #44 |
| Windows C runtime | Ships the CRT DLLs beside the server and refuses to build without them (PACKAGING.md:641-656) | **absent**. SNAP has no `.cargo/config.toml` (`ls` found none) and nothing sets `crt-static`. The planned test is `tc4_exe_imports_no_vcruntime` (TC4-ISSUES.md:394) | G1 | Windows with MSVC (the VM is enough) | none | A start on a clean machine |
| Windows signing | PACKAGING.md:545-633; package-desktop.yml:256-264, 312-319 and 405-426; sign-windows.ps1 (162 lines); D99 (DECISIONS.md:2706-2750) | **absent** (TC4-ISSUES.md:395, #456) | G1 | A GitHub-hosted Windows job with `id-token: write` (package-desktop.yml:262-264). There is no stored secret (D99 ruling 7) | The federated credential's subject is pinned to `repo:unfoldingWord@6601424/translationCore4@1323470539:environment:windows-signing` (PACKAGING.md:602). The rebuild's repository is birchamp/tc-rst, so it needs a new credential or must release from the unfoldingWord org (b6). It also needs the repository variables at PACKAGING.md:621-627 | PERFORMANCE.md:114 ("a signed, installed Windows build"); the D99 release gate |
| macOS arm64 pkg | build-macos-pkg.zsh:14-30; package-macos.zsh:73-86 (bundle id `org.unfoldingword.translationcore4` at :75; an ad-hoc codesign at :83-86); PACKAGING.md:333-401. **Not notarized** (PACKAGING.md:9, :356; D99 ruling 1, DECISIONS.md:2716-2718) | **absent**. No macOS run appears in docs/evidence | G1 through #44 and #214 (TC4-ISSUES.md:392, :396). The G1 row names only a Windows runner (APP-DESIGN.md:3425) | macos-15, or a Mac | Is macOS in scope (b1)? Notarization would need an Apple Developer ID, which tC4 does not use | The P8 Graphite probe on macOS (PLAN.md:278-281) |
| Linux x64 zip | PACKAGING.md:403-487 (`start-tc4.sh`, and the `--no-sandbox` fallback at :442-463); package-desktop.yml:161-244 | **absent** | G1 (Linux frame, #642: TC4-ISSUES.md:355) | ubuntu-24.04 with xvfb, or this container (/usr/bin/xvfb-run is present) | Is Linux in scope (b1)? | The P8 probe on Linux (PLAN.md:280-281) |
| Electronite production main script: supervision, single instance, prep window, buffer mirror | desktop-main.cjs:1-183 (AppUserModelId :106; the lock :108-111; `second-instance` :134-140; `openExternal` :124-131; PDF :18-30; downloads :38-49; keychain :51-96). desktop-prepare.cjs:1-64 (the worker ends before the lock is released, :9-18) | **absent**. Only the spike exists: spikes/electronite/shell/main.cjs:3 says "Throwaway: never shipped", and src/main.rs:1-9 is a stand-in server. The design is in AD11 (APP-DESIGN.md:193), AD21 (:203), AD39 (:221) and §5.1 (:1610-1640) | G1, which waits for D2 | Build and test here under Xvfb. G1's "done when" needs a Windows runner (:3425) | Q-1 is decided: the filter (:3554). Q-2: the shell sends feedback (:26). Q-3 follows its recommendation, the `keyring` crate in the server (:32, :3556). The help-desk secrets (tC4 package-desktop.yml:123-125) are needed. **The docs contradict each other on how the shell starts tc4:** :71 says plainly, :221 and :3425 say detached | PERFORMANCE.md:116 (a window within 1 s); #4; P8 |
| Bundling tc4 serve with the wasm UI | package-desktop.zsh:279-487 (assembling the app); :485 (product.json); BUILD-MANIFEST.json (PACKAGING.md:296-298) | **Partial**: crates/tc-ui/build-app.sh:1-24 (the UI folder; `TC4_BUILD_COMMIT` at :21-22); `tc4 serve --app-data --ui --bundle` (crates/tc4/src/args.rs:36-38); crates/tc-api/src/app.rs:12-17. The gate `cargo test --release -p tc-api --test refusals` must run before an installer is built (APP-DESIGN.md:565-567). There is no staging script, no manifest and no Electronite pin in code: the sha256 is only in .audit/tasks/g0-windows.md:66-77, for win32 | G1 (#278: TC4-ISSUES.md:361) | Each target OS | Where app data lives in production. The shell must pass `--app-data` (args.rs:47), and §4.9 (APP-DESIGN.md:1544-1568) names no path | Every packaged test |
| OBS image pack fetch, checked against fetched.lock | The pin is at package-desktop.zsh:159 and PACKAGING.md:936 | **No downloader.** `git show resume-build-lane:bundle/en/fetched.lock`, lines 27-36, holds the commit `7146d5b5…`, sha256 `d8170e18…` and size 37,766,473, with `"file": null`. bundle_store skips the pack (crates/tc-app/examples/bundle_store/build.rs:71-79). bundle/en/README.md:92-98 is out of date: it says "This folder has no `fetched.lock` yet". Those lines date from 3310b89b (2026-10-16); the lock was added in f0aee5b (2026-10-18) | **none**. The ruling at rebuild.tsv:40 says only "packaging step is built with tc-app" | Any runner that can reach git.door43.org, which is blocked here (PLAN.md:318) | none | OBS pictures available offline at first run |
| Third-party notices | package-desktop.zsh:582-634 (licenses/, THIRD-PARTY-NOTICES.md, COPYING); the shipped smoke test checks them (smoke-installed.zsh:96-98) | **absent**. The inputs exist: COPYING at the root, crates/tc-door43/NOTICE:1-5 (shipping it is required for Windows builds) and crates/tc-align/NOTICE. evidence/proxy-watch-audit-2026-10-19.md:208 says "tc-rst has no way to ship third-party notices yet" | G1 (TC4-ISSUES.md:357, :397); About is A5 and G1 (:261) | Any; this container works | none | GPL, MIT and ISC obligations; #554 and #606 |
| Build-time smoke test | package-desktop.zsh:703-1074: a deliberately wrong resource path, 303 then 200, the version guard (:864-889), the second launch (:936-958) and the store guard (:1060-1073); check-zip-product.zsh | **absent**. The parts exist in crates/tc-ui/app/smoke.mjs and prepare-smoke.mjs, but CI does not run them (ci.yml:55-72) | G1 (#45: TC4-ISSUES.md:427; the second-launch smoke test in the G1 row, :3425) | Each OS | none | #4 and #45 |
| Post-install smoke test | smoke-installed.zsh (338 lines) and smoke-installed.ps1 (238 lines); package-desktop.yml:361-588 (install, signatures, and reinstall and uninstall keeping the project at :434-455) | **absent** | G1 | A fresh runner for each OS | none | #44, including "uninstall keeps projects" (TC4-ISSUES.md:396) |
| Offline run | PACKAGING.md:837-911: done by a person and recorded; on Linux it can be scripted with `unshare -rn` (:862-865) | **absent** | G1 (#43: TC4-ISSUES.md:426) | A person on a clean machine; the Linux namespace version can be scripted | none | tC4's tag rule |
| P8 Graphite probe on Windows and macOS | PLAN.md:276-281 and :322-324. tC4 has not proven this in its packaged app either (PACKAGING.md:330-331) | **Only a smoke test.** Awami Nastaliq measured 302.9 px wide with an ascent of 61 px on Linux (evidence/electronite-spike-linux-2026-10-17.md:239-251) and joined on Windows too (APP-DESIGN.md:80-81). Both say "not the P8 probe". No probe of vertical extent exists | G1 (#32: TC4-ISSUES.md:429) | Electronite on Windows and macOS; Linux here | Is macOS in scope (b1)? | P8 |
| Start-up budget runs | PERFORMANCE.md:114-116 and :118-167 | **absent**. The count tests exist (APP-DESIGN.md:3161) | G2 (:3426) and G1 (#743, #751, #740: TC4-ISSUES.md:344, :347, :348) | The reference laptop: 4 cores, 8 GB, Defender on (evidence/windows-machine-2026-10-17.md:6-23) | The signing credential (b6), and whether a laptop measurement counts as a "CI check" (b8) | PERFORMANCE.md rows 114 to 116 |
| App identity and the way tC4 users move over | tC4 uses `APP_NAME="translationCore4"` (package-desktop.zsh:175), the AppId above, and the store `$HOME/pankosmia/tc4-projects` (PACKAGING.md:75, :89, :658) | The projects folder is `~/translationCore4/projects` (crates/tc-app/src/config.rs:17-19, :133-137; Q-4, APP-DESIGN.md:22-23). Outside help, evidence and research notes, no rebuild code or doc names `tc4-projects` (grep). TC4-REQUEST-MOVED-NOTICE.md:34-35 says the first release fixes the "app id … name and download page". The help pages ship byte-pinned (AD29, :211): st-where-projects-live.html:129-131 says to back up `~/pankosmia/tc4-projects`, and gs-install.html:129 points to the translationCore4 releases and `start-tc4.sh` | **none** | — | Decisions b2 to b5 and b9 | STORAGE.md:4019-4029, where stragglers "update"; how a translator finds tC4 projects to import (P9) |

## (b) Owner decisions, with the exact question

1. **Platforms.** "Does 'done' include the macOS arm64 pkg and the Linux x64 zip, as tC4 4.0 ships them (PACKAGING.md:16-20, :1093-1095)? Or is it Windows x64 only, with P8's macOS probe dropped (PLAN.md:280)? If macOS is in, does it stay unsigned and not notarized, as in D99 ruling 1?"
2. **Identity.** "Does the rebuild install over tC4 4.0 or beside it? Over it means the same name `translationCore4`, the same Inno AppId `{24B10730-…F242}` and the same id `org.unfoldingword.translationcore4` (tc4.iss:13, :50; package-macos.zsh:75). Beside it means a new name and ids."
   - *Inference:* Over it, tC4 is removed when the rebuild installs, so a teammate who has not moved yet cannot use tC4 for a last sync. Beside it, both apps can run at the same time.
3. **Version.** "What version does the first release carry? The workspace is `0.1.0` (Cargo.toml:9); tC4 is `4.0.0-rc.2` (PACKAGING.md:1057-1063)."
4. **tC4 4.0 projects at first launch.** "Should the first launch look in `~/pankosmia/tc4-projects/_local_/_local_/*` (on Windows, `%USERPROFILE%\pankosmia\tc4-projects`) and offer to import what it finds?"
   - Today Home shows a tC4 4.0 project only if someone copies it into the rebuild's folder, and then as "Cannot be opened" (APP-DESIGN.md:1356, :1370-1371).
   - Import step T0 needs the tC4 store's root folder, `<repos>` (TC4-DELTA-2026-10-16.md:342-347).
   - *Inference:* if the answer to 2 is "over it" and this answer is no, a tC4 user opens the rebuild and sees "No projects yet". Their work would be hidden from them.
5. **The help page on where projects live.** "The rebuild ships tC4's help page byte for byte. It says projects live in `~/pankosmia/tc4-projects` and to back up that folder (st-where-projects-live.html:129-131), but the rebuild uses `~/translationCore4/projects`. Do we replace that page as a declared difference from AD29, or move the folder?"
6. **The signing credential (D99).** "Who adds an Entra federated credential for the rebuild's repository and a GitHub environment that may sign? Or does the release pipeline run in an unfoldingWord-org repository?" The existing subject is fixed to translationCore4 (PACKAGING.md:597-617).
7. **Runners.** "May the release workflow use GitHub-hosted `windows-2025` and `macos-15` runners (the ones tC4 uses), or `windows-latest` and `macos-14`, on birchamp/tc-rst? Or must the Windows jobs run on your Windows 11 VM as a self-hosted runner?" Q-26 says only that "the release pipeline will have a Windows machine" (APP-DESIGN.md:26-27).
8. **The start-up gate.** "PERFORMANCE.md:114 needs a signed, installed build on the reference laptop with Defender on. Is a run there, through a self-hosted runner or a recorded script, the 'CI check that can fail' for this row?"
9. **Distribution.** "Where do translators download the rebuild: the releases of which repository? And what is the 'Get {name}' page that the tC4 notice opens (TC4-REQUEST-MOVED-NOTICE.md:44-52)? Does tC4 4.0.x ship that notice?"
10. **The help-desk secrets.** "Do the rebuild's release builds get tC4's `TC_HELP_DESK_TOKEN` and `TC_HELP_DESK_EMAIL` (package-desktop.yml:123-125), since Q-2 has the shell send feedback?"

## (c) Where each part can be built and tested

**This Linux container:**
- The UI build (build-app.sh), `tc4` for Linux, and bundle_store, which skips the OBS pack.
- The notices generator and the license checks.
- The Linux zip, its staging and its build-time smoke test.
- The shell's own tests, and Electronite's Linux build under Xvfb as root with `--no-sandbox` (PLAN.md:322-323; Linux spike evidence :3-4).
- The P8 probe on Linux.
- The release gate `cargo test --release -p tc-api --test refusals`.
- Possibly the offline run under `unshare -rn`. I did not check that it works here.

Limits:
- The rebuild pins no Electronite binary. One exists outside the repository, at /home/bot/Enjoy/tC4r-cdx/proof/app/electronite/electron, version not checked.
- git.door43.org is blocked here (PLAN.md:318), so the OBS download cannot run here.
- Nothing can be built until the heavy-test slot is free.

**The owner's Windows 11 VM** (.audit/tools/winvm: SSH to 127.0.0.1:2222; it has MSVC, Git, Node 22 and Rust, per setup.ps1):
- It can build `tc4.exe` with MSVC and check the CRT test.
- With Inno Setup installed, it can build an unsigned installer and run the silent install, reinstall and uninstall smoke tests.
- It cannot sign: signing needs GitHub OIDC (D99 ruling 7).
- *Inference, not checked:* an Electronite window may not open in an SSH session with no desktop.

**The owner's reference laptop** (windows-machine-2026-10-17.md): the first-launch, repeat-launch and window-on-screen budgets on a signed, installed build with Defender on, and the manual offline run.

**A Mac, or a macos-15 runner:** the pkg, the ad-hoc codesign, the frame (#214) and the P8 probe on macOS. No Mac appears anywhere in the rebuild's evidence.

**A GitHub-hosted Windows runner:** signing, and the `smoke-windows-x64` equivalent (fresh profile, signature checks).

## (d) Proposed unit split

Each unit is done when a CI job that can fail passes.

- **G1a Shell.**
  - Scope: the main script, AD11, AD21, AD39 and §5.1; the prep window; `printToPDF` behind a stub until D2; dialogs; `openExternal`. It ports the tests of desktop-prepare.
  - Done when: `shell-linux` (ubuntu-24.04 with xvfb) and `shell-windows` (windows-2025) both pass these cases:
    - a second launch exits and starts no second `tc4`;
    - after `tc4` is killed, Home comes back;
    - after the shell is killed, `tc4` exits within 5 s;
    - after a hard kill, the mirror still has the typed text;
    - a same-origin help link stays in the app.
  - Fix the plain-versus-detached contradiction (APP-DESIGN.md:71 against :221 and :3425) first.
- **G1b Staging, the Linux zip, notices and the OBS fetch.**
  - Scope: one script that builds `tc4` and the UI with `TC4_BUILD_COMMIT`, downloads Electronite pinned by sha256 (tC4's three sums are at package-desktop.zsh:119-122), and downloads the OBS pack, failing when it differs from fetched.lock. It also runs bundle_store with no skips, writes the notices from Cargo.lock, the NOTICE files, the fonts, Chromium's licenses and the resources, writes BUILD-MANIFEST.json, and runs the refusals release gate.
  - Done when: `package-linux-x64` fails in each of these cases:
    - a pack sha256 differs from fetched.lock (a negative control);
    - a crate has no notice;
    - the build-time smoke test fails.
  - Also fix bundle/en/README.md:92-98, which still says the folder has no fetched.lock.
- **G1c Windows installer and signing.**
  - Scope: an Inno script using the identity from decision b2, the CRT fix (#284), and signing only on `workflow_dispatch` from a protected branch.
  - Done when: `smoke-windows-x64` installs silently, checks the shortcuts and signatures, runs the installed smoke test, and shows that reinstalling and uninstalling keep a project byte for byte (as tC4 does at package-desktop.yml:361-462).
- **G1d macOS, only if decision b1 includes it.**
  - Scope: the .app with an ad-hoc seal, the pkg, and the frame (#214).
  - Done when: `smoke-macos-arm64` installs the pkg and passes the installed smoke test.
- **G1e Installed smoke test and offline run.**
  - Scope: the smoke test ships with the app. It creates a project, writes a verse, restarts, reads the verse back, shows an OBS picture with the internet switch off, checks that a Burrito export validates (P5), checks the store path, and checks the second launch.
  - Done when: `smoke-<os>` jobs on fresh runners pass, plus an `offline-linux` job under `unshare -rn`. A person's offline witness is recorded for each release.
- **G1f P8 Graphite probe.**
  - Scope: a probe page that measures the width, ascent and descent of Graphite fonts inside the packaged Electronite.
  - Done when: `graphite-probe-{linux,windows,macos}` pass, with macOS only if b1 includes it.
- **G1g Identity and migration.**
  - Scope: one source for the name and ids; a first-launch scan of tC4 4.0 projects if b4 says yes; and the help-page difference from b5.
  - Done when: `first_launch_lists_tc4_projects_for_import`, using a fixture home that holds `pankosmia/tc4-projects`, passes, and the release smoke test checks the store path.
- **G2** stays as it is. It adds a start-up budget run on the reference laptop once G1c produces a signed build (decision b8).

**Inference:** only G1a's PDF bridge needs D2. G1b and G1g can start now.

## Findings
- [RP1] (critical/high) No branch of the rebuild has a release pipeline. Every branch has only ci.yml, and that file runs only on ubuntu-latest.
  - evidence: `git ls-tree -r --name-only <branch> .github` lists only .github/workflows/ci.yml on claude/focused-ride-5uzj7p, resume-build-lane, s2-proof-executor and storage; SNAP .github/workflows/ci.yml:11-12, 47-48, 65-66 (three jobs, all ubuntu-latest); TC4 .github/workflows/package-desktop.yml:1-588 (tC4's release pipeline: a preflight job, three build jobs and three smoke jobs)
  - rec: Create units G1b, G1c and G1e (staging, installer and signing, installed smoke), each done when a CI job that can fail passes.
- [RP2] (high/high) G1 is the only owner of release work, and its design row leaves most of that work out. The row names only a Windows runner and no installer, signing, notices, OBS fetch, macOS or Linux, although TC4-ISSUES.md gives G1 the issues #44, #456, #554 and #606. G1 also waits for D2.
  - evidence: SNAP docs/APP-DESIGN.md:3425 (the G1 row); SNAP docs/TC4-ISSUES.md:355, 357, 391, 392, 394, 395, 396, 397, 404, 426, 427, 429; SNAP .audit/STATUS.md:74 (G1 to do, waits for D2)
  - rec: Split G1 into G1a to G1g as in section (d) of the report. Only the PDF bridge needs D2.
- [RP3] (high/high) The app's identity is not decided, and the moved-project notice designed for tC4 needs it at the first release: the rebuild's app id, name and download page.
  - evidence: SNAP docs/TC4-REQUEST-MOVED-NOTICE.md:29-35 and 44-52; TC4 scripts/package-desktop.zsh:175 (APP_NAME=translationCore4); TC4 scripts/tc4.iss:13 (the AppId) and :50 (the AppUserModelID); TC4 scripts/package-macos.zsh:75 (the bundle id); SNAP Cargo.toml:9 (version 0.1.0)
  - rec: Ask the owner questions b2, b3 and b9. Keep the name and ids in one place in the code (G1g).
- [RP4] (high/high) The help the rebuild ships tells translators that their projects are in ~/pankosmia/tc4-projects and to back up that folder. The rebuild keeps projects in ~/translationCore4/projects, so a translator who follows the help backs up the wrong folder.
  - evidence: SNAP crates/tc-ui/help/st-where-projects-live.html:129-131; SNAP crates/tc-ui/help/gs-install.html:129 (points to the translationCore4 releases and start-tc4.sh); SNAP crates/tc-app/src/config.rs:133-137; SNAP crates/tc-ui/tests/help_pages.rs:1-4 (the pages are pinned byte for byte, AD29); SNAP crates/tc-server/src/lib.rs:341 (/help/ is served)
  - rec: Ask the owner question b5. Replace the page as a declared difference from AD29, or add a pinned override, and test that the help names the folder the app actually uses.
- [RP5] (high/high) Nothing finds tC4 4.0 projects. No rebuild code names tc4-projects. Home shows a tC4 project only if it was copied into the rebuild's folder, and then as unusable. Import step T0 needs tC4's store root, and the only real one is ~/pankosmia/tc4-projects.
  - evidence: grep of 'tc4-projects' in SNAP crates, docs and .audit: matches only the vendored help, evidence/memory-compare-windows-2026-10-19.md:132 and the research notes; SNAP docs/APP-DESIGN.md:1356 and 1370-1371; SNAP docs/TC4-DELTA-2026-10-16.md:342-347; TC4 docs/PACKAGING.md:75, 89 and 658
  - rec: Ask the owner question b4. If the answer is yes, G1g adds a first-launch scan with a fixture home. Inference: if the rebuild installs over tC4, a missing scan hides the translator's projects.
- [RP6] (high/high) tC4's Windows signing works only from tC4's repository. Its Azure OIDC federated credential is bound to unfoldingWord/translationCore4's windows-signing environment, and the rebuild's repository is github.com/birchamp/tc-rst.
  - evidence: TC4 docs/PACKAGING.md:583-627 (the subject at :602); TC4 docs/DECISIONS.md:2706-2750 (D99; ruling 7: no stored secret); `git -C /home/bot/tc-rst remote -v` gives origin https://github.com/birchamp/tc-rst
  - rec: Ask the owner question b6. Until it is answered, G1c builds unsigned, and the start-up budget at PERFORMANCE.md:114, which needs a signed build, cannot be measured.
- [RP7] (medium/high) The rule to download the OBS image pack at packaging time exists only on paper. fetched.lock holds the pack's sha256 and size, but nothing downloads the pack, bundle_store always skips it, and bundle/en/README.md:92-98 still says the folder has no fetched.lock.
  - evidence: `git show resume-build-lane:bundle/en/fetched.lock` lines 27-36 (file null, sha256 d8170e18…, size 37766473); SNAP crates/tc-app/examples/bundle_store/build.rs:71-79; SNAP .audit/rebuild.tsv:40; `git blame` shows README:92-98 from 3310b89b (2026-10-16); the lock was added in f0aee5b (2026-10-18); SNAP docs/PLAN.md:318 (git.door43.org is blocked here)
  - rec: G1b adds a download step that checks the pack against fetched.lock and fails on a mismatch, with a negative control, and makes bundle_store refuse to skip anything in a release build. Fix the README lines.
- [RP8] (medium/high) The rebuild has no way to ship third-party notices, although a NOTICE file says Windows builds must carry proxy-watch's MIT notice. COPYING and two NOTICE files exist.
  - evidence: SNAP crates/tc-door43/NOTICE:1-5; SNAP crates/tc-align/NOTICE; SNAP docs/evidence/proxy-watch-audit-2026-10-19.md:208; SNAP docs/TC4-ISSUES.md:357, 397; TC4 scripts/package-desktop.zsh:582-634; TC4 scripts/smoke-installed.zsh:96-98
  - rec: G1b generates THIRD-PARTY-NOTICES from Cargo.lock, the NOTICE files, the fonts, Electronite's and Chromium's licenses and the resources. The release check fails when a crate has no notice.
- [RP9] (high/high) P8 needs the Graphite probe on Windows and macOS, but no Mac or macOS runner exists anywhere in the rebuild, the platform scope is not decided, and the vertical-extent probe has not been written. The G0 results are smoke tests only.
  - evidence: SNAP docs/PLAN.md:276-281, 322-324; SNAP docs/evidence/electronite-spike-linux-2026-10-17.md:239-251 ('not the P8 probe'); SNAP docs/APP-DESIGN.md:80-81; TC4 docs/PACKAGING.md:330-331 (tC4 has not proven Graphite in its packaged app either)
  - rec: Ask the owner question b1. G1f writes the probe and runs it on Linux here and on windows-2025 and macos-15 runners.
- [RP10] (high/high) The start-up budgets can only be measured on a signed, installed Windows build with Defender on, on the reference laptop. No such build can exist until the installer and signing do.
  - evidence: SNAP docs/PERFORMANCE.md:114-116, 133-138; SNAP docs/evidence/windows-machine-2026-10-17.md:6-23; SNAP docs/APP-DESIGN.md:3426 (G2)
  - rec: Order the work G1c, then the G2 laptop run. Ask the owner question b8: does a self-hosted run or a recorded run count as the CI check?
- [RP11] (low/high) The design contradicts itself on how the shell starts tc4. The G0 results say G1 starts tc4 plainly; AD39 and the G1 row say detached.
  - evidence: SNAP docs/APP-DESIGN.md:71 ('so G1 starts `tc4` plainly'); SNAP docs/APP-DESIGN.md:221 (AD39: detached: true); SNAP docs/APP-DESIGN.md:3425 ('start `tc4` detached'); SNAP spikes/electronite/shell/main.cjs:126-131
  - rec: Settle one wording before G1a starts. G0 measured that both ways leave no process behind.
- [RP12] (medium/medium) Nothing makes the Windows tc4.exe start on a machine without the Visual C++ runtime: there is no crt-static setting and no shipped DLLs. tC4 needed a fix for this (#284).
  - evidence: `ls -la SNAP/.cargo` found nothing; grep for crt-static or vcruntime in SNAP crates, .github and Cargo.toml found nothing; SNAP docs/TC4-ISSUES.md:394 (planned test tc4_exe_imports_no_vcruntime); TC4 docs/PACKAGING.md:641-656
  - rec: G1c adds `-C target-feature=+crt-static` for the MSVC release build, or ships the DLLs, plus the import test. Inference: Rust's MSVC target links the CRT dynamically by default.
- [RP13] (medium/high) Release checks that must pass before an installer is built are not in CI. Browser smoke tests skip in CI, and the release-mode refusals gate never runs.
  - evidence: SNAP .github/workflows/ci.yml:55-72; SNAP docs/APP-DESIGN.md:565-567; SNAP crates/tc-ui/app/smoke.mjs, prepare-smoke.mjs (they exist)
  - rec: G1b and G1e run `cargo test --release -p tc-api --test refusals` and the browser smoke tests in the packaging jobs.
- [RP14] (medium/high) The rebuild's code pins no Electronite version or checksum. The win32 sha256 appears only in a task document; tC4 pins all three platforms.
  - evidence: SNAP .audit/tasks/g0-windows.md:66-77; TC4 scripts/package-desktop.zsh:119-122 (the darwin-arm64, linux-x64 and win32-x64 sha256 sums); grep for the sums in SNAP code: they appear only in the task document
  - rec: G1b records the Electronite v37.1.0-graphite tag and the three sums in the packaging script and fails on a mismatch.

## Unverified
- Whether github.com/birchamp/tc-rst is private, whether it has GitHub Actions minutes, and whether macOS runners may be used. api.github.com is blocked here (SNAP PLAN.md:318). Check with `gh repo view birchamp/tc-rst` and the repository's Actions settings.
- Whether the same product name would make the rebuild share tC4 4.0's Electron userData folder and single-instance lock. This depends on the name in desktop-app-template 4cb7576's electron/package.json, which I did not clone. Read that file at that commit.
- Whether v4.0.0-rc.2 was published as a GitHub release, which would mean tC4 4.0 users with data already exist. The tag exists in the clone, but I could not read the releases API.
- Whether an Electronite window can open on the Windows VM when it is started over SSH, which has no desktop session. This is an inference. Test it by launching electron.exe through .audit/tools/winvm/run.sh.
- Whether this container can download Electronite from github.com today, and whether `unshare -rn` works here. I ran no network call and no namespace test.
- Inno Setup's upgrade-in-place behaviour with the same AppId, and Rust MSVC's dynamic CRT default, come from general knowledge, not from files I read in this task.
- The current machine's disk use: `df -h /home` shows 76%, not the 97% the brief gives. The 97% may be another partition. I did not check further.
