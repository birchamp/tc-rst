# Plan for design unit B9b: share steps and Open from Door43

Worktree `.claude/worktrees/lane-b9b`, branch `lane-b9b`, cut from `claude/focused-ride-5uzj7p`
at `cda7057`. Row (docs/APP-DESIGN.md §10): share target and check steps against the fake; R12
(count and the fake's route); M6 form with `ScriptedOpenDoor43`; shared state on cards; A8's
note (capture `share-internet-consent` and `open-door43-internet-consent` whole, `share-signin`'s
pixels from the card, delete its `rust` route). Tests: `openFromDoor43Dialog` port;
internet-consent share and open cases. Done when: the 6 screens of §7.6 are in the report.
No Q-n waits; Q3 and Q-21 are answered (Q-21 by its recommendation, "Owner's answers").
Issues: #724 (team-access line for every project, OBS included, Q2), #120 (account and write
calls on `--door43`; reads never), #372 (J11 steps up to the check step; never force a push).

Steps, each a commit once its crate's tests and clippy pass:

1. **R12.** `Door43Client::count_repositories(session, owner, lang)`: `GET /repos/search
   ?owner&lang&limit=1`, the count is `X-Total-Count`, a missing or non-numeric header is an
   error. The fake answers `/api/v1/repos/search` from the repositories it holds (owner without
   case, `lang` exactly, visible to the caller) with `X-Total-Count`. `parity_account` gives the
   parity organizations tC4's counts (Equipo-Ejemplo 4 es-419, es-419_gl 12), checked against
   `fake-door43.mjs`. Push refusals stay as they are (HTTP 200, declined ref, Step 6).
2. **Server.** tc-api: `account/orgs {language}` [share] answers the user and each organization
   with whether a repository can be made there and whether it is Recommended (tC4's
   `recommendOrganization`, pure, its cases ported); `account/repoLink {url}` [repoLink] gives
   back a URL on `--door43` only; `door43/open {request, repo, name}` [open] (job, `Join`):
   the name checked against the listing without case (`open-door43.name-exists` before
   anything), then the open through `ScriptedOpenDoor43`, which answers a scripted outcome
   (test builds) or `Unavailable(Sync)` until X/E1b. The source and name rules (tC4's
   `openSourceOf`, `openDefaultName`, `openRepoPathOf`) live once in tc-api for both sides.
   `door43/shareTargets` is `account/orgs`; `door43/shareCheck` is dropped (tC4's check step
   asks Door43 nothing; its facts are the card's). tc-app tests against the fake.
3. **UI.** `tc_ui::app::share` (tC4 `sh`) and `tc_ui::app::open_door43` (tC4 `od`), pure, with
   host tests (the `openFromDoor43Dialog` port's rules); the views ShareDialog (target, check,
   and the progress, done and error drawing E1b feeds; the team-access line for every project)
   and OpenFromDoor43; `Pending::Share` runs (sign-in on `NeedsSignIn`, then the dialog),
   `Pending::OpenDoor43`, `LinkKind::Repo`; Add a project's fourth card opens M6. Q3's key
   replaces `openDoor43.where` in all ten languages (each tC4 translation without its second
   sentence); Q-21's key replaces `openDoor43.refused.other` (pending outside English).
4. **Parity.** `units.json` gains B9b; capture the six screens; `q3-where` gets its `rust`
   target; `share-signin`'s `rust` route goes; report rewritten and every row reviewed.
   Journeys: the share halves of internet-consent a, b, d up to the check step, and the open
   question (Not now sends nothing; Turn on internet runs the open).
5. **Docs.** The B9b row's refinements, §3.9, §6.7 and §7.6 notes.
