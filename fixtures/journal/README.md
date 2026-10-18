# Journal fixtures for `tc-journal`

Tests read these files read-only. Regenerate them with the commands below; do not edit the
generated files by hand.

| File | What it is | How it was made |
|---|---|---|
| `v0-every-kind.ndjson` | 64 change lines of format minor 0 covering all 18 record kinds, every cause and the escapes of canonical JSON. The test decodes each line and encodes it again, so any change to the wire form (which would change every device's change ids) fails. | `TC_JOURNAL_BLESS=1 cargo test -p tc-journal --test canonical every_kind_has_the_pinned_wire_form` |
| `newer-minor.ndjson` | 11 change lines as a newer app (minor 1) and a malformed minor-0 writer would write them: unknown fields at three levels, an unknown record kind with and without a value, an unknown cause, a known cause with an unknown source, an unknown verse value variant, an unknown book code, and the same unknowns at minor 0. The comments in the script say what each line is. | `python3 -I fixtures/journal/make_newer_minor.py` |
| `format-1.0.json` | `format.json` exactly as this app writes it for a new project. | `TC_JOURNAL_BLESS=1 cargo test -p tc-journal --test versioning` |
| `format-1.3-newer.json` | A newer minor's `format.json` that older apps may still edit: an extra derivation, an extra derived path and an unknown field. Hand-written. | by hand |
| `format-1.4-read-only.json` | A newer minor that changes a derivation, so `minReaderMinor` is 4. Hand-written. | by hand |
| `format-2.0.json` | A newer major. Hand-written. | by hand |

All files are in the public domain.
