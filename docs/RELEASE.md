# Releasing translationCore 4

How the app is built, tested and released for Windows, macOS and Linux (card G3; owner rulings
H3, H4, H10 and H16). The pipeline is `.github/workflows/release.yml`; its scripts are in
`scripts/release/`.

## What a release is

- **Version:** semver, from `[workspace.package] version` in `Cargo.toml`. The first public
  release is 4.1.0, tag `v4.1.0` (H16 part 1). A patch fixes, a minor adds, a major breaks an older
  project or sync peer.
- **Names (H16 part 3):** the app shows itself as **translationCore**; it is installed as
  **translationCore 4** (install folder, Start menu, Applications). The download page is
  https://translationcore.com/tc4.
- **Platforms (H3):** Windows 10 x64 and later, macOS 13 and later on Apple silicon, Ubuntu 22.04
  x64 and later. The runners that build and test them are windows-2025, macos-15 and ubuntu-24.04.
- **Files:** `translationCore4-<version>-windows-x64.exe` (`-unsigned` when not signed), a per-user
  installer that needs no administrator; `translationCore4-<version>-macos-arm64-unsigned.pkg`;
  `translationCore4-<version>-linux-x64.zip`; and one `BUILD-MANIFEST-<platform>.json` each.
- **BUILD-MANIFEST.json** (also inside each installed app): the commit, the version, the platform,
  Electronite's tag and archive sha256, and the sha256 and size of every shipped file. About shows
  the same commit (`app/hello`'s `build.commit`), and the smoke test checks they match.

## Where things live on the translator's computer

| What | Where |
|---|---|
| The app | Windows `%LOCALAPPDATA%\Programs\translationCore 4`; macOS `/Applications/translationCore 4.app`; Linux the unzipped `translationCore 4` folder |
| Projects | `translationCore4/projects` in the home folder (H16 part 2) |
| App data: resources, settings, drafts, logs | `app-data` in the shell's user-data folder: Windows `%APPDATA%\translationCore4`, macOS `~/Library/Application Support/translationCore4`, Linux `~/.config/translationCore4` |

Uninstalling or updating replaces only the app. Projects and app data stay (tC4 #44); the smoke
test checks it.

## Cutting a release

1. On the default branch, when the release is ready, the owner (or someone the owner names)
   pushes the tag: `git tag v4.1.0 && git push origin v4.1.0`. Nobody else pushes release tags.
2. The tag runs `release`. It builds nothing for a tag while any interface string is untranslated
   (job `translations`, `cargo test --release -p tc-api --test refusals`; rulings/2026-10-22-H10.md).
3. Each platform is built, installed on a fresh runner of its own system and smoke-tested there.
   Only when every job passed does `publish` make a **draft** GitHub release with the installers
   and manifests.
4. The owner reads the draft, adds the release notes (what the translator gets, from the merge
   subjects since the last tag), and publishes it.

A tagged build is unsigned unless signing is set up (below). To sign a release, run the workflow
by hand on the default branch with **sign** ticked, after the tag's run passed.

## Signing (H4)

Yakob signs, as for tC4. Windows is signed with Azure Artifact Signing through GitHub's OIDC token,
exactly as tC4's `package-desktop.yml` (#456): no password or key is stored in this repository.
macOS stays unsigned and not notarized, as tC4 4.0 rc.2; the app is ad-hoc signed so Apple
silicon runs it, and the pkg's README says how to open it (System Settings, Privacy & Security,
Open Anyway).

What must be set up once in the GitHub repository, by someone with admin rights (owner action A4):

- An environment named **`windows-signing`**, with a deployment branch rule that allows only the
  default branch.
- Five repository variables (Settings, Secrets and variables, Actions, **Variables**; they are not
  secret): `AZURE_SIGNING_ENDPOINT`, `AZURE_SIGNING_ACCOUNT`, `AZURE_SIGNING_PROFILE`,
  `AZURE_TENANT_ID`, `AZURE_CLIENT_ID` — the same values tC4's repository uses.
- In Azure, on the app registration `AZURE_CLIENT_ID` names: a federated credential for GitHub
  Actions with organization and repository of **this** repository, entity type *Environment*,
  environment `windows-signing` (subject `repo:<owner>/<repo>:environment:windows-signing`).

A manual run with **sign** ticked on the default branch then signs every `.exe`, `.dll` and
`.node` file of the app, the installer and its uninstaller; files Microsoft already signed keep
their signature. Any other run builds unsigned and its file names say `-unsigned`. Check a
signature in PowerShell:

```powershell
Get-AuthenticodeSignature .\translationCore4-4.1.0-windows-x64.exe | Format-List Status,SignerCertificate,TimeStamperCertificate
```

## Secrets

- `TC_HELP_DESK_TOKEN` and `TC_HELP_DESK_EMAIL` (repository secrets, the same as tC4's): the
  help-desk key the shell uses to send feedback (docs/contracts/SHELL.md item 11; H15). A build
  without them still works but cannot send feedback.

## Testing the pipeline from a branch

A push that changes `.github/workflows/release.yml` or `scripts/release/` runs every build and
smoke test on that branch, unsigned, and publishes nothing. Once the workflow is on the default
branch, it can also be run by hand from the Actions tab on any branch.

## The checks on every release

| Check | Job | What it proves |
|---|---|---|
| `translations` | translations | no interface string ships untranslated (H10) |
| `release_has_no_test_hooks` | build | the shipped `tc4` has no test flag, route or text |
| `tc4_exe_imports_no_vcruntime` | build (Windows) | `tc4.exe` needs no Visual C++ runtime (tC4 #284) |
| Electronite sha256 | build | the shell is the pinned Electronite v37.1.0-graphite |
| `installed_manifest_is_the_build_s` | smoke | the installed files are the build's |
| `app_starts_and_starts_tc4` | smoke | the installed app starts with no arguments and brings up tc4 |
| `smoke.mjs` steps | smoke | address line, About's commit, resources offline, a project made, a verse saved and read back after a restart, one server per app-data folder |
| `uninstall_keeps_app_data`, `uninstall_keeps_the_projects_folder` | smoke | removing the app keeps the translator's work (tC4 #44) |

## What the app does not do

The app does not check for new versions (tC4 #259). Translators get a new version from the
download page.
