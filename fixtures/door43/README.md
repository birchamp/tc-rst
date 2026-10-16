# Door43 API fixtures

Tests read these files read-only. Do not edit them by hand.

## `swagger-definitions.json`

The answer and request schemas the Door43 client and the fake Door43 server use, cut
from the DCS (Gitea) API specification.

- **Source:** `swagger.v1.json` in https://github.com/unfoldingWord/uW-Tools-Collab at
  commit `c3d42d9b76e80ea7b01fa889aa4b037e5ad8fd54` (2026-07-24). The source file's
  sha256 is `0bfb37dd33c1b74122c287ed0f1b54efe7e6a078e4088a926a6bf81a93007dd7`.
- **API version:** `1.23.7+dcs+12-ge61c163c9c`, base path `/api/v1`.
- **What was kept:** the `info` and `basePath` fields and these definitions, unchanged:
  `APIError`, `AccessToken`, `CatalogEntry`, `CatalogSearchResults`, `CatalogStage`,
  `CatalogStages`, `Commit`, `CommitMeta`, `CommitUser`, `CreateAccessTokenOption`,
  `CreateRepoOption`, `Ingredient`, `Organization`, `OrganizationPermissions`,
  `PayloadCommitVerification`, `Permission`, `Release`, `RepoCommit`, `Repository`,
  `Tag`, `User`. Keys are sorted and indented with two spaces.
- **License:** MIT, © 2023 unfoldingWord (the uW-Tools-Collab `LICENSE`). The
  specification itself states `"license": {"name": "MIT"}`.

To regenerate, load the source JSON, keep `info`, `basePath` and the definitions named
above, and write them with `json.dump(..., indent=2, sort_keys=True)` plus a final
newline.

The values in the specification are examples, not Door43 data. No recorded live Door43
answers exist yet; `git.door43.org` is blocked from the build environment. Record them
from `qa.door43.org` when a machine has access, and add them here with their own
provenance.

## `fetched-lock/`

A `fetched.lock` in the format of `bundle/en/fetched.lock`, beside the one archive it names,
for the tests of `tc_resources::FetchedLock`. Nothing in it came from Door43.

- `demo-e8c6503832022a8fd38bb426781d6b6127dde664.zip` is the `git archive` of a three-file
  repository made for this fixture with git 2.43.0: `README.md`, `content/1.json` and the
  executable `run.sh`, committed as `fixture <fixture@example.org>` at
  `2026-10-16T00:00:00Z`, then `git archive --format=zip --prefix=demo/` of that commit. Its
  commit is `e8c6503832022a8fd38bb426781d6b6127dde664` and its tree, from
  `git rev-parse HEAD^{tree}`, is `6e5802a33cc85441708e9d4dc17c71c2b25b6cc0`. The zip is 748
  bytes, sha256 `ed0c325b58e18976d29b70151f704548df297f285ca0710704cbe1def4f4de51`.
- The lock's second entry, `fixture/images`, is invented. It stands for an archive the
  packaging step downloads (`"file": null`), so no test checks its tree or digest.
