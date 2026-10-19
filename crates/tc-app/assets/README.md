# Files compiled into tc-app

## `langnames.json`

Door43's language list, as translationCore 4 ships it for the language suggestions of New
Bible and New Open Bible Stories (tC4 #492; APP-DESIGN AD27). `tc_app::language_search`
compiles it in and ranks it on the server. The app never fetches it: the suggestions work
offline.

- Source: tC4 `src/data/langnames.json` at commit `d310bf3` (the rebuild's reference), byte
  for byte. tC4 refreshes it from Door43's list with `scripts/refresh-langnames.mjs`.
- Git blob id: `dc3db76abb73002532d428fcad596c1895583bdd` (1,006,246 bytes, 9,166 rows).
  `the_language_list_is_tc4s` (`crates/tc-app/tests/app/language_search.rs`) checks it.
- To vendor it again from a newer tC4 reference:

  ```sh
  git -C <tC4 checkout> show <commit>:src/data/langnames.json > crates/tc-app/assets/langnames.json
  ```

  then put the new commit and blob id here and in that test, and run the language-search tests:
  the ranking cases are tC4's own, and the code offer counts (Q35) move with the list.
