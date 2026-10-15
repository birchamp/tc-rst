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
