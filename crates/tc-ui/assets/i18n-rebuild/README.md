# Rebuild-only interface text

The catalogs in `../i18n` are tC4's, byte for byte. Text that only the rebuild shows lives
here, in the same flat form: a dotted key to a message with `{name}` placeholders. The UI reads
a language's file after its tC4 catalog, and a key here never repeats a key of a tC4 catalog.

- `en.json` holds every rebuild key in English.
- `<id>.json` holds the keys translated into language `<id>`.
- `<id>.pending.json` lists the keys not yet translated into `<id>`. Until a key is translated,
  that language shows it in English.

`crates/tc-api/tests/refusals.rs` checks these files: every key of `en.json` is translated or
pending in every language, with the placeholders of the English text. A release build fails
while any key is pending (`release_has_no_pending_translation`), so a screen cannot ship in
English in another language unnoticed (APP-DESIGN §3.5, Q-10). To add a translation, put the
key in `<id>.json` and take it out of `<id>.pending.json`.
