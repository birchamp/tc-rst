# Request to tC4: say where a project moved

For the translationCore 4 team. 2026-10-16. Status: proposed; the rebuild's side is unit X of
the storage plan.

## Why

The owner chose a one-way move from tC4 4.0 to the rebuild (`docs/JOURNAL-DECISION-2026-10-16.md`,
"Decisions"). After a team moves, the Door43 repository holds the rebuild's files under
`ingredients/tc/` and no longer holds `ingredients/checking/journal/`. A teammate who still runs
tC4 4.x then fails to sync: its verify step finds files it does not derive and refuses with
`sync.verify-failed`, which reads "The app found a problem in the combined project, so it did not
send it to Door43. Nothing changed. Ask for help." Nothing is overwritten, but the message reads
like damage.

The owner's ruling: tC4 4.0.x may say the project moved **only if it knows where it moved to**.
Otherwise it keeps today's refusal.

## What the rebuild writes

The commit that moves a team writes one file, once, and never rewrites it:
`ingredients/tc/moved.json`, listed in `metadata.json` like any ingredient.

```json
{
  "format": "tc-moved",
  "version": 1,
  "from": "translationCore 4",
  "app": "<the rebuild's application id>",
  "date": "2026-10-16"
}
```

- `app` is a stable application id. The rebuild's first release fixes it, with the app's name
  and download page, and gives all three to the tC4 team.
- The file names no person. (A username in public history is a known tC4 problem, L28.)
- The file is UTF-8 JSON with these five keys. A reader ignores keys it does not know.

## What tC4 4.0.x would do

1. **Trigger.** After a sync fetches Door43 `main`, and before any other check (before G2), tC4
   reads `ingredients/tc/moved.json` if it exists. "Open from Door43" does the same after its
   clone.
2. **Knowing where.** tC4 ships a table of application ids it knows, each with a name and a
   download page. The table starts with the rebuild's id. tC4 shows the notice only when the
   file parses, `format` is `tc-moved`, `version` is 1, and `app` is in that table. It takes the
   name and the page from its own table, never from the repository, so nobody with write access
   to a repository can make tC4 show a link.
3. **The notice.** The sync stops before it commits or pushes anything and changes nothing on
   Door43 or on this computer. tC4 says: "This project moved to {name}. Your teammates now use
   {name}. Your work since your last sync is still on this computer and comes along when you
   open the project in {name}." It offers one button, "Get {name}", which opens {page} in the
   browser.
4. **Not knowing where.** If the file is missing, cannot be parsed, has another `format`, a
   `version` other than 1, or an `app` tC4 does not know, tC4 behaves exactly as today
   (`sync.verify-failed`).

## Tests the change needs

- A repository fixture with `moved.json` naming a known id: the notice shows, with the name and
  page from tC4's table; nothing is committed, pushed or deleted; local unsynced work stays.
- The same fixture with an unknown id, a malformed file, `version` 2, or no file: today's refusal,
  unchanged.
- A fixture whose `moved.json` holds an extra key and a URL: the notice uses tC4's own page and
  never the repository's URL.

## The rebuild's side

Unit X specifies `moved.json` in `docs/STORAGE.md` §13.3, writes it in the move commit, and tests
that a later sync never rewrites or deletes it. The §13.3 bridge still converts any tC4 segments
a straggler pushes after the move, so no straggler's work is lost either way.
