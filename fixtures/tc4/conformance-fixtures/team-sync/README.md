# The team-sync fixture

This fixture holds the journals of three translators, A, B and C, on one project. It is the
shared fixture of team sync round 1 ([D98](../../../docs/DECISIONS.md), epic
[#24](https://github.com/unfoldingWord/translationCore4/issues/24)). The conformance checks
(JC-35), the sync core tests (#675), the conflict review tests (#679) and the journey (#681)
use its cases. When you find a sync bug, add a case here first. Then fix the bug.

Do not edit the files by hand. Change `generate.mjs`, then run `npm run generate` in
`conformance/`. JC-35 builds the fixture again and fails when the files differ.

## Layout

| Path | What it holds |
|---|---|
| `journals/<actor>/` | One journal folder for each translator: `actor.json` and `segments/`. Together they are the state after all three synced. Every file is valid. |
| `cases/invalid-file/` | One broken segment of C. It is not in `journals/`. |
| `cases/changed-own-file/` | A different file at the path of one of A's own segments. It is not in `journals/`. |
| `cases.json` | The index of the cases below as data: each case name, its files, and its register key. |
| `generate.mjs` | The generator. It uses the reference writers only (the §8.8 seed and `writeActionSegment`). |

The actors are `translator-a`, `translator-b` and `translator-c`. Their Door43 usernames are
`fixture-translator-a`, `fixture-translator-b` and `fixture-translator-c`. These names
match the Door43 grammar and name no real account.

## Cases

Find a case by its name in `cases.json`.

| Case | What happened | What the fold shows |
|---|---|---|
| `creation` | A creates the project from the conformance sample (Titus and Jonah, with check results and alignments). This is one seed segment. B and C base their work on it. They do not seed again (D53). | Both books project. |
| `names` | A, B and C each write their own Door43 username once (`actor.name.set`, `v: 3`). | `names` maps each actor to its username. `namesRejected` is empty. |
| `different-verses` | A changes Titus 1:1, B changes Titus 1:2, and C changes Titus 1:3. | No fork. All three verses project. The Titus 1:1 alignment becomes invalid, because its text changed (I-3). |
| `verse-two-ways` | A and B change Titus 1:4 from the same base. | A verse-text fork on `text\|TIT\|1:4`. B is the newest head, so B's text shows, flagged provisional. This is the fork for the conflict review. |
| `check-two-ways` | B sets one translationWords check result to `valid`. Then C sets the same check result to `todo`. | A non-text fork on the decision key. C is the newest head, so `todo` projects (G7). |
| `invalid-file` | A segment of C whose body does not match its `sha256`. | The one validator refuses it (`checksum`). A sync that meets it refuses (G2). |
| `changed-own-file` | A different valid action at the path of A's own Titus 1:1 segment. | A sync that meets it on Door43 refuses and changes nothing (G1). |

Not yet a case: an alignment set two ways (a non-text fork on an `align|` key). The sync core
(#675) adds it here first, with its test.

## How to use a case

- To model Door43 `main`, copy `journals/` and remove the files that a translator has not
  shared yet.
- To model a broken incoming file, copy the case file into the same actor folder of that
  copy.
- To fold the fixture, read `journals/` with `readUnion` (`journal/files.mjs`).
- The base `metadata.json` and the resource records are those of
  `conformance/sample-burrito/`, as `sampleInputs()` in `generate.mjs` reads them.
