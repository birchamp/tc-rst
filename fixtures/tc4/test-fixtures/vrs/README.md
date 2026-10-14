# Versification scheme fixtures

The six schemes the platform ships, copied verbatim from
`pankosmia/resource-core` `templates/content_templates/vrs/`.

These are byte-identical to the upstream Copenhagen Alliance specification's
standard mappings (`versification-mappings/standard-mappings/`), unchanged
upstream since 2025-06-18. Verified 2026-08-24 by SHA-256:

| scheme | sha256 (first 12) |
|---|---|
| eng | `5162023f8e06` |
| org | `3b8e065fb592` |
| lxx | `90d980f78451` |
| rsc | `c4a8ce2551d4` |
| rso | `93fbb63369c2` |
| vul | `e09ea002e89a` |

Kept in-repo so the tests do not depend on a sibling `dev-env/` checkout.
See `docs/evidence/versification-format-and-frames-2026-08-24.md`.
