# Local task: record Door43 facts that the cloud session cannot reach

Run this in a Claude session on a computer that can reach `git.door43.org`, in a clone of
`github.com/birchamp/tc-rst`. The cloud session that builds the rebuild cannot reach Door43.
Read `AGENTS.md` first; its rules apply.

Start from branch `claude/focused-ride-5uzj7p` (pull it first), create a new branch
`door43-facts`, commit there, and push it. Do not push to `claude/focused-ride-5uzj7p`: the
cloud session pushes there and will merge your branch.

Read first: `bundle/en/README.md` (the sections "Shipped: the git archive of each commit-only
pin" and "Fetched when the app is packaged"), `crates/tc-door43/src/verify.rs`
(`verify_archive(zip, commit, commit_tree, max_unpacked)`, `verify_sb_export`) and
`crates/tc-door43/src/client.rs` (the Door43 client, `download_archive`, `download_sb_export`,
`tag_commit`, `commit_tree`).

## 1. Tree ids of the two bundled lexicons

`bundle/en/` holds the Door43 git archives `en_ugl-d9d29e2d589258ce27f92b59f753a3af03ab7a72.zip`
(`uW/en_ugl` at `d9d29e2d589258ce27f92b59f753a3af03ab7a72`) and
`en_uhl-72df5ac25acf9d51e826b20e3ad883a5a657ef4e.zip` (`uW/en_uhl` at
`72df5ac25acf9d51e826b20e3ad883a5a657ef4e`). Their burrito metadata cannot vouch for their
contents, so the archive's file tree must equal the commit's tree.

For each: get the commit's tree id from Door43 (for example
`GET https://git.door43.org/api/v1/repos/uW/en_ugl/git/commits/<commit>`, which gives
`commit.tree.sha`), cross-check it a second way (for example a shallow fetch of the commit and
`git rev-parse <commit>^{tree}`), and confirm the bundled zip matches it with
`tc_door43::verify_archive`. Do not compute the tree by unpacking with another tool: file
modes can be lost on Windows and macOS.

## 2. The OBS image pack

Download the git archive of `uW/obs_images_360` at `7146d5b504f6b63b9e11f7dc0b18c594d0ae179d`
(`https://git.door43.org/uW/obs_images_360/archive/7146d5b504f6b63b9e11f7dc0b18c594d0ae179d.zip`),
get its tree id the same way, check it with `verify_archive`, and record its sha256 and size.
Do not commit the zip: the release pipeline downloads it when it packages the app.

## 3. Write `bundle/en/fetched.lock`

UTF-8 JSON with stable key order:

```json
{
  "format": "tc-fetched-lock",
  "version": 1,
  "entries": [
    { "repo": "uW/en_ugl", "commit": "<40 hex>", "tree": "<40 hex>", "file": "en_ugl-<commit>.zip", "sha256": "<64 hex>", "size": 0, "checked": "<YYYY-MM-DD>", "how": "<the API call and the second method>" },
    { "repo": "uW/en_uhl", "commit": "<40 hex>", "tree": "<40 hex>", "file": "en_uhl-<commit>.zip", "sha256": "<64 hex>", "size": 0, "checked": "<YYYY-MM-DD>", "how": "<the API call and the second method>" },
    { "repo": "uW/obs_images_360", "commit": "7146d5b504f6b63b9e11f7dc0b18c594d0ae179d", "tree": "<40 hex>", "file": null, "sha256": "<64 hex of the archive>", "size": 0, "checked": "<YYYY-MM-DD>", "how": "<the API call and the second method>" }
  ]
}
```

## 4. An offline test that checks the bundle against the lock

Add a test in `crates/tc-resources/tests/` (or `crates/tc-door43/tests/`, whichever fits
better). It never touches the network. It parses `bundle/en/fetched.lock` into typed values
and, for each entry with a `file`, checks that the zip's sha256 equals the lock's and runs
`verify_archive` with the lock's commit and tree. It fails if a lexicon archive in `bundle/en/`
has no lock entry. Check that it fails when one byte of a copy of a zip changes. Update
`bundle/en/README.md` to say the tree ids are recorded and checked offline.

## 5. Live check of the Door43 client (read-only)

Add a small example binary (for example `crates/tc-door43/examples/door43_live.rs`; `cargo
test` never runs it) and run it once against the real Door43 with the client's internet switch
on. It must:

1. Look up the tag commit of `unfoldingWord/en_tn` tag `v91`. Expect
   `e586762e330f482a60c52aedd1c7b3a2f155df8a`.
2. Download and verify the burrito export of `unfoldingWord/en_twl` at `v91`. Expect revision
   `7cbb3419f79780ce84b92ad7d42bd476cc5d8a64`.
3. Download and verify the git archive of `uW/en_ugl` at
   `d9d29e2d589258ce27f92b59f753a3af03ab7a72`.
4. Run one catalog search (for example `unfoldingWord` `en_ult`).

Write the outcome to `docs/evidence/door43-live-<date>.md`: each request, its status code and
how long it took. If the real server shows that the fake server
(`crates/tc-door43-fake`) got something wrong, fix the client and the fake, with a
regression test against the fake.

## 6. Optional, only if you are comfortable doing it: Door43's real 403

The rebuild maps HTTP 403 on a push to "no write access", but nobody has seen Door43's real
reply (tC4 issue #710). If you have a Door43 account and a repository you can read but not
write, try to push a throwaway branch to it with your token, and record the exact status,
headers and body in the same evidence file. Never record the token.

## Done when

`cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings` and
`cargo test --workspace` pass on branch `door43-facts`, the branch is pushed, and the final
message lists the three tree ids, the image pack's sha256 and size, and the live results.
