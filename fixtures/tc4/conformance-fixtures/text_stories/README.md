# fixtures/text_stories — the vendored OBS content template

This directory is a byte-for-byte copy of pankosmia's `text_stories` content template:
`templates/content_templates/text_stories/` in `pankosmia/resource-core` at commit
`54802be780af18ab02e426dd59014bc6adb158af` (the rig's pin, `dev-env/scripts/setup-from-pins.zsh`,
2026-08-14). Vendored 2026-09-15 for the conformance harness (BURRITO-SPEC §10, issue #147).
55 files: `metadata.json` (with its `%%…%%` placeholders), `ingredients/LICENSE.md`,
`ingredients/content/01.md` to `50.md`, `content/front/title.md`, `content/front/intro.md`,
`content/back/intro.md`. Tree checksum (`find . -type f | sort | xargs md5 -q | md5 -q`):
`786e47a532540fb8e93b60438d19f4ca`, equal to the rig's assembled copy at
`dev-env/app-resources/templates/content_templates/text_stories/` on 2026-09-15.

The platform copies `ingredients/` of this template into every new OBS repository
(`POST /git/new-obs-resource`, `src/endpoints/git2/new_obs_resource.rs` [VERIFIED —
pankosmia-web 0.18.5 (99fd9be, 2026-07-30)]). `generate-obs.mjs` builds
`sample-burrito-obs/` from it in the §10 seed form, and the `OBS` group of `validate.mjs`
checks layout and `currentScope` equality against it.

Content licence: unfoldingWord® Open Bible Stories, © unfoldingWord, CC BY-SA 4.0
(`ingredients/LICENSE.md`). Do not edit these files. To refresh them, copy the template
from the platform again and record the new commit, date and checksum here.
