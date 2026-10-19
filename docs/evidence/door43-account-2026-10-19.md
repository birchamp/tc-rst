# The Door43 account and the internet question (unit A8, 2026-10-19)

Unit A8 builds the account service against Door43, the sign-in step (M12) and the account
menu's Door43 rows (docs/APP-DESIGN.md §10, A8). This file holds its measurements. No
PERFORMANCE budget covers them; they show that A8 adds no request where tC4 has none and no
round trip a person waits for.

The machine is the owner's Linux PC (`docs/evidence/linux-machine-2026-10-18.md`): a 12th Gen
Intel Core i7-12700, 20 threads, SHA instructions yes (`sha_ni`), AVX-512 no. **It was heavily
loaded**: other agents' builds and test runs and a virtual machine ran at the same time, and the
load average was 135 to 191 during the timings (A5's were taken at 3.3). The timings below are
therefore upper bounds; the request counts do not depend on load. The raw summaries are beside
this file, in `door43-account-2026-10-19/`.

The code measured is branch `p0-a8` at `55022e8`. Builds: `crates/tc-ui/build-app.sh --release`
(the `wasm-release` profile) and the dev build, served by the debug `tc4` of `cargo build -p tc4
--features test-hooks,fixture-backend`, signing in to the debug `tc-door43-fake` binary on
loopback, in Chromium 141.0.7390.37 (Playwright 1.56.1).

Command: `TC4_BIN=target/debug/tc4 TC4_UI=<build> TC4_DOOR43_FAKE=target/debug/tc-door43-fake
node crates/tc-ui/app/measure-account.mjs --runs 20 --out
docs/evidence/door43-account-2026-10-19/<build>.json`.

## Door43's requests per action

Read from the fake Door43's own log (its control port), the same on both builds:

| Action | Requests to Door43 |
|---|---|
| open the account menu, open the sign-in step | none |
| sign in, no earlier token of the name | `GET /api/v1/user`, `GET /api/v1/users/{user}/tokens`, `POST /api/v1/users/{user}/tokens` |
| sign in again (a `translationCore` token exists) | `GET /api/v1/user`, `GET …/tokens` twice, `DELETE …/tokens/{id}`, `POST …/tokens` |
| sign out | none |
| check a saved sign-in (resume) | `GET /api/v1/user` with the kept token |

tC4 makes the same requests but one: when a token exists, it lists the tokens once with
`?limit=50` (`tC4:src/data/share/door43Api.ts:299-300`), where tc-door43's paging reads a second
page to be sure that a server which caps its page below the limit asked for has nothing more
(`crates/tc-door43/src/client.rs:639-665`). One request more per sign-in, against the fake on
loopback about 2 ms; tc-door43's rule is kept.

## A sign-in (the click on Sign in to the account button's `data-state="in"`)

Measured in the page, from the click's own time stamp to the mutation that sets the account
button's state, the internet on (so no question), twenty sign-ins each followed by a sign-out:

| 20 runs | median | p95 | max |
|---|---:|---:|---:|
| release | 27.4 ms | 76.5 ms | 96.0 ms |
| dev | 24.7 ms | 40.7 ms | 49.3 ms |

The server's part, timed from Node with `account/signIn` alone on the same loaded machine: a
median of 14.3 ms for its three requests to the fake (one fake request about 2.4 ms). The rest is
the page's request, the reply's handling and the event frame. The release and dev figures do not
differ beyond the machine's noise at this load.

## Opening the sign-in step from the account menu

From the click on "Sign in to Door43" to the step in the DOM, and to the next frame:

| 20 runs | median | p95 | max |
|---|---:|---:|---:|
| release, to the DOM | 16.5 ms | 36.3 ms | 51.4 ms |
| release, to the next frame | 18.9 ms | 38.6 ms | 57.0 ms |
| dev, to the DOM | 24.1 ms | 79.6 ms | 155.9 ms |
| dev, to the next frame | 29.2 ms | 85.4 ms | 158.0 ms |

Opening the step sends nothing (no API request, journey "d. sign-in from the menu asks when the
password is sent"), so this is the page's own work: the menu closing and giving the focus back,
and the dialog mounting. A5 measured the App language dialog at a load of 3; a figure for M12 at
that load is left to the next quiet run of this script.

## Bytes

`tc_ui_bg.wasm`, release: 2,282,096 bytes, where A5 recorded 2,176,892: A8 adds 105,204 bytes
(4.8 %), the sign-in step, its rules and the account actions. Between the two, tc-ui's own
source changed only with A8; tc-api, which the UI compiles in, also took C0's small changes
(`write.rs`, the `test/saveVerse` rows), so a few hundred of those bytes may be C0's.
