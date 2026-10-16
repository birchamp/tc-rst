# The OBS content template

`text_stories/` is pankosmia's `text_stories` content template:
`templates/content_templates/text_stories/` in `pankosmia/resource-core` at commit
`54802be780af18ab02e426dd59014bc6adb158af`, as tC4 vendors it in
`conformance/fixtures/text_stories` (this repository's
`fixtures/tc4/conformance-fixtures/text_stories`, whose README gives the provenance). The
files are byte for byte that copy, without its README. A new OBS project is made from them
(`tc_obs::template`).

`metadata.json` keeps resource-core's placeholders (`%%ABBR%%`, `%%CONTENT_NAME%%`,
`%%CREATED_TIMESTAMP%%`, `%%LANGUAGE%%`) and lacks `localizedNames`, which
`tc_obs::template::with_localized_names` adds when a project is made, as tC4's
`scripts/fix-obs-template.mjs` does when it serves the template.

The test `the_shipped_template_is_the_pinned_one` (`crates/tc-obs/tests/tc4_template.rs`)
checks every file against tC4's copy with tC4's own validation rules. Do not edit these
files. To refresh them, copy the template again, record the new commit here and in the
fixture's README, and update the fixture.

Content licence: unfoldingWord® Open Bible Stories, © unfoldingWord, CC BY-SA 4.0
(`text_stories/ingredients/LICENSE.md`).
