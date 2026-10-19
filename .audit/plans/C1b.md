# C1b: offers and read-only modes (builder's plan, 2026-10-19)

Row (APP-DESIGN §10): draft offers at open (`drafts.json`, buffered entries and the shell's
mirror, merged and filtered, §6.6); unverifiable drafts as text; read-only banners;
`SecondWriter`; the copy offer, which carries the folder's drafts to the copy. Waits for R20
and R7's copy command: both are on main (`KeepDraft` of an unverified draft writes no parents;
`Store::copy_project` copies `drafts.json`), so no fallback is built. No Q-n gates the row.

B3 (`session/open`, the open job) is not on main, so the offer is a query of its own that the UI
sends once an open has its scope (and again after a reload while the offer stands). No server
state: an offer carries what Keep sends back.

1. tc-api `session`: `session/drafts { at, buffered }` -> `OfferedDrafts { offers, saved }`;
   `session/keepDraft { at, request, kind, shown, text, discard }` -> `KeptDraft`;
   `session/discardDraft { at, request, stored }`; `session/copy { at, request }` ->
   `ProjectCopy`. Golden files.
2. tc-app backend: a draft token that parses verified or unverified; `StoreCommand::keep_draft`;
   `ProjectHandle::discard_draft`; `ProjectStore::copy`; the stored drafts' tokens as wire text.
3. tc-app `drafts`: the merge (pure over a snapshot): entries of this folder and id only; the
   newest per field and run (the UI's merge); a stored draft gives way to a buffered entry of its
   field at least as new; an entry whose text the stored value or its own base already has goes to
   `saved`; distinct texts of one field are each offered; an entry whose `sent` text the stored
   value equals is kept over the stored value's token (A4b's rule); a token that fails its MAC
   is `AsVersion`; a field no save exists for yet (a section, a new comment) is `CopyText`.
   A read-only session offers nothing (STORAGE §5.14), so the entries wait for the copy.
4. Handlers and the copy (named "<name> - Copy", then "- Copy (2)" ...).
5. Tests (store on FaultFs, through `App::call`): `relaunch_offers_buffered_and_mirrored_entries`,
   `draft_with_unverifiable_token_is_offered_as_text` through the app, the crash tests through
   `test/crash` (end the server during a save, relaunch, offered, Keep makes a conflict when the
   verse changed; the sent rule keeps without one), discard, and the copy after a second writer
   carries the drafts.
6. tc-ui: the banner per reason and the offer's words and settle step as pure functions, with
   the rebuild keys in every locale (pending where untranslated).
7. Design text: §3.9 rows, §6.6 notes, the row's note.
