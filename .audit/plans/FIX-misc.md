# Plan for fix unit FIX-misc: three tC4 issues in existing code (#701, #552, #549)

Planner's revision 1, 2026-10-19. Commit this file with your first commit. It is the only file the
planner wrote. (The planner also compiled a throwaway probe crate from its scratchpad into this
worktree's `target/` to check `tc_burrito::build_metadata`; that left build artifacts only.)

Worktree `/home/bot/tc-rst/.claude/worktrees/app-fmisc`, branch `app-fmisc`, cut from `tc4fix`
at `c1bfa78` ("Map tC4's newer changes and issues onto the rebuild"), the same commit as
`claude/focused-ride-5uzj7p`. Sibling fix units cut from the same `tc4fix`: `app-ftext` and
`app-fhelps`. Before you start: `git -C /home/bot/tc-rst log --oneline -3 tc4fix`. If tc4fix has
moved, merge it into `app-fmisc` first.

FIX-misc is a fix unit, not a design unit. It has no row in APP-DESIGN §10.
`docs/TC4-ISSUES.md` is its spec: §1 rows 12, 13 and 15, plus the area rows at lines 204, 262,
324 and 326. Each item and its required regression test:

| # | tC4 issue | Where | Regression test the spec names | Fix the spec gives |
|---|---|---|---|---|
| 12 | #701 | `crates/tc-resources/src/gateways.rs:122-132` | `ur_deva_helps_read_left_to_right_and_are_named_in_devanagari` | Left to right and उर्दू. Record the difference and tell the owner. (B3 prefers the direction in the installed helps' manifest.) |
| 13 | #552 | `crates/tc-ui/src/ds/forms/select.rs` | ds-gallery case `Select/disabled-while-open` | Return early in `choose` when disabled, and close the list when it becomes disabled. Check the label-press edge in the same case. |
| 15 | #549 | `crates/tc-obs/src/images.rs:168-183`, `:300-309` | `a_project_picture_without_a_role_is_the_project_rung` | Find project pictures by path under `ingredients/` (outside `checking/`, `tc/` and `content/`), matched by file name. Declared difference. |

The orchestrator also asked for a test over the whole gateway registry (direction against script)
and for the #549 test to use a carried picture. Both are in the steps below.

What the planner read: AGENTS.md; docs/PLAN.md, ARCHITECTURE.md and PERFORMANCE.md (all of
each); APP-DESIGN "Owner's answers", §0 to §2, §5.3 and §5.5, §7.1 to §7.5, the §10 introduction,
and the B3, B7 and B9a rows; TC4-ISSUES §1 and §2 to §8; TC4-DELTA-2026-10-19 §1, §3.1 and §5.
On the storage branch (read with `git show storage:docs/STORAGE.md`): STORAGE §3.1 to §3.4.
Rebuild code: `gateways.rs` and `lib.rs` in tc-resources; `images.rs`, `presentation.rs`,
`ids.rs` and `template.rs` in tc-obs, with its tests `tc4_obs.rs`, `vectors.rs` and
`common/mod.rs` and the vectors `fixtures/obs/obs-vectors.json`; `select.rs`, `field.rs` and
`layer.rs` in tc-ui `ds/`; tc-ui's `gallery/{render,props,components}.rs`,
`tests/gallery_record.rs` and `Cargo.toml`; tc-burrito `build.rs`; the tc-store
`materialize.rs` carried-file listing (lines 1005-1016); the tc-journal `fold/project.rs`
`role()`. In `parity/ds-gallery`: README, `cases/{lib,forms}.mjs`, `build-spec.mjs`,
`protocol.mjs`, `diff.mjs`, `dom-diff.mjs`, `scenarios.mjs`, `capture-rust.mjs`,
`src/render.jsx` and `vite.config.mjs`. The p0-a5 branch's diff to tc-ui and the app branch's
diff to docs. tC4 at `d310bf3`: `src/data/gateways.ts`, `src/data/obsImages.ts`,
`src/data/obsStory.ts:147-159`, `src/ds/components/forms/Select.jsx`,
`src/ds/components/primitives/Layer.jsx`, `docs/BURRITO-SPEC.md` (S-2, R-10.2.4 and R-10.6.2),
`test/obsResources.test.ts`. Leptos and tachys 0.8 sources: event delegation, listener removal
and MaybeProp conversions.

---

## 1. What FIX-misc does, and what it does not

Does:

1. **#701.** In the gateway registry, `ur-deva` reads left to right and is named उर्दू. A test
   checks every registry entry's direction against the script of its autonym and its language
   tag. This is a declared difference from tC4 4.0.
2. **#552.** When a Select turns disabled while its list is open, the list closes and no choice
   is taken. The ds-gallery gets two cases, `Select/disabled-while-open` and
   `Select/disabled-while-open-label`. Both differ from tC4 on purpose. `residuals.json`
   declares the difference and a cargo test checks it. This is a declared difference from tC4 4.0,
   which still has the bug at `9ad4d5c`: `git diff d310bf3 9ad4d5c -- src/ds` is empty.
3. **#549.** tc-obs finds a story project's own pictures by path, not by role. A test builds a
   carried picture into a `metadata.json` written by the rebuild's own writer, which lists it
   with no role. This is a declared difference from tC4 4.0.
4. **docs/PLAN.md.** Add one bullet for each of the three differences under "Where the rebuild
   differs from tC4 4.0 on purpose".

Does not, with the reason for each (repeat these in your final answer):

- **B3's preference for the direction in the installed helps' manifest** (the last sentence of
  row 12) belongs to B3, which is not built. tc-helps already parses that direction:
  `ResourceIdentity.direction` from the Burrito `languages[].scriptDirection` or the RC
  `dublin_core.language.direction` (`crates/tc-helps/src/metadata.rs:219-222`, `:286-288`). A
  doc line on `Gateway::direction` points B3 at it.
- **The Source texts parity screen.** `parity/tc4-baseline/dom/source-texts.json` draws the
  ur-deva row as `<span dir="rtl">"اردو"`. The rebuild's version of that screen will show
  उर्दू, left to right. B9a, which captures `source-texts`, masks that row and cites the PLAN
  entry. The PLAN bullet says so. Nothing is captured for that screen today.
- **A Devanagari interface face (#613)** is A5's work. Until A5 lands it, उर्दू draws in a
  system face.
- **TC4-ISSUES.md and TC4-DELTA-2026-10-19.md stay as they are.** Their counts tables and
  adjacent §1 rows are shared with every fix unit (rows 11, 14 and 16 belong to other units), so
  edits there would conflict. The integrator updates the statuses at merge. Your final answer
  lists the three items as handled. It also says that #552 is not in DELTA §5's "Differences to
  tell the owner", so the owner must be told of it.
- **`.audit/rebuild.tsv`** is written by the orchestrator at merge.

---

## 2. Decisions (put each in the commit message that makes it)

- **D1 (#701).** Change the entry to `autonym: "उर्दू"` (U+0909 U+0930 U+094D U+0926 U+0942) and
  `Direction::Ltr`, with a comment that tC4 lists right to left and اردو. The registry-wide test
  decides direction from the autonym's letters: Hebrew, Arabic, Syriac, Thaana, N'Ko and their
  supplements and presentation forms count as right-to-left letters. When the language tag
  carries a BCP 47 script subtag (`ur-deva` → `deva`), every letter of the autonym must be in
  that script. A subtag the test's table does not know fails the test, so anyone who adds an
  entry like that must add its script too. This check catches tC4's entry both ways: Arabic
  letters under a `deva` tag, and right to left for a Devanagari name.
- **D2 (#549).** The path rule lives where ingredients become picture assets:
  `project_image_assets` plus a new `pub fn is_project_picture(key)`. A picture found this way
  gets `role: Some("x-obsimages")`. **`resolve_image` is unchanged**: it keeps tC4's
  `resolveObsImage` rule that a project asset counts only with that role. The committed tC4
  vectors (`fixtures/obs/obs-vectors.json`, 26 cases, including "project without role →
  missing") pin that rule, and they keep passing. TC4-ISSUES points at `:300-309` because that
  filter is what blocks a carried picture today. With D2 it no longer blocks one, and the rung
  order stays a faithful port. Say this in the commit message.
- **D3 (#549).** The rule: the key starts with `ingredients/`, and the file name ends in `.jpg`,
  `.jpeg`, `.png` or `.webp` (ASCII case ignored; the existing `is_picture_name`, the same set
  as tC4's `obsImagePackFromPaths`). The **first** folder below `ingredients/` must not be
  `checking`, `tc` or `content`. A file directly in `ingredients/` qualifies. Only the first
  folder decides: `ingredients/pictures/content/x.jpg` is a project picture. Folder names
  compare exactly, because the app writes its own folders in lower case. The role is ignored on
  purpose. tC4's BURRITO-SPEC rule S-2 says "paths are authoritative; roles are decorative".
  STORAGE §3.3 lists carried files with no role and says that no reader depends on a role.
- **D4 (#549).** The signature changes, since nothing calls it yet (grep finds no caller in this
  branch, `app`, `p0-a5` or `app-c0`):
  `project_image_assets<'a>(ingredient_keys: impl IntoIterator<Item = &'a str>, uri_for: impl Fn(&str) -> String) -> Vec<ImageAsset>`.
  It takes the same kind of input as `pack_from_ingredients`.
- **D5 (#552).** Make two code changes in `Dropdown`:
  1. `choose` returns at once when `disabled()` is true.
  2. A new `Effect` runs `open.set(false)` when `disabled()` turns true while the list is open.
     It does **not** call `close()`. `close()` would send the focus to a button that is about to
     be disabled, and whether that succeeds depends on the order in which the render effects
     run. The focus goes to `<body>` with the removed panel, as a browser's focus fixup does for
     a control that turns disabled.
- **D6 (#552).** The gallery cases turn the Select off in a click's **capture phase**: a wrapper
  `div` has `onClickCapture: setTo('off', true)`. The click that chooses a row turns the Select
  off before the row sees the click. This is like a screen that turns a form busy at that
  moment. A `mousedown` trigger fails: the opening click would disable the field first. The
  cases open the list by keyboard (`focus` the combobox, then `ArrowDown`).
- **D7 (#552).** The gallery gate accepts a declared difference. It accepts it only when
  `residuals.json` names its ruling (which docs/PLAN.md must list), its layers (`pixels`,
  `dom`), why, and the exact pairs of DOM lines that differ (tC4's line, the port's line).
  `gallery_record.rs` fails in these cases:
  - a residual explains a case that no longer differs (so a port that falls back to tC4's bug
    fails);
  - the DOM differs in any line the residual does not declare (so a port that leaves the list
    open fails);
  - the ruling is missing from PLAN.

  This mirrors the `screen_record` rules of APP-DESIGN §7.5 for `parity/app/residuals.json`.
  `diff.mjs` already reads `parity/ds-gallery/residuals.json` (README "Layout"). Until now only
  `gallery_record.rs` was stricter than the README.
- **D8.** Each PLAN bullet goes in the commit that makes its difference. The gallery test checks
  that `(#552)` is listed.

---

## 3. Facts the code must respect

- **Leptos event delegation is off.** tc-ui enables only `leptos/csr`, and delegation is the
  opt-in `delegation` feature (leptos 0.8.22 `Cargo.toml`, tachys 0.2.19). So `on:click` adds a
  native listener on the element. Dropping a view removes its listeners: `RemoveEventHandler`'s
  `Drop` runs the remover (tachys `src/renderer/mod.rs:145-149`). A listener removed during a
  dispatch is not called.
- **Effects run at the microtask checkpoint** after each native listener returns. In the gallery
  case, the wrapper's capture listener sets the cell, and the close effect then removes the list
  before the click reaches the row. So once the effect exists, the `choose` guard cannot be
  reached from a browser. The guard covers a disable and a choice in one task, and any later path
  that skips the effect. Step 3 proves it with a mutation check, not with a case.
- **tC4's React still takes the choice.** Its root capture listener runs `onClickCapture`. React
  flushes the update (the button is disabled; the list stays, because tC4 has no
  close-on-disable). Its bubble listener then calls the row's `onClick` → `choose(o)`, which
  never reads `disabled` → `onChange({target:{value:'EST'}})` → `close()`. `close()` cannot
  focus the disabled button, so the focus leaves with the panel. tC4 ends with the list closed,
  the field disabled and the caption "Chosen: EST". The port ends with the same, except
  "Chosen: none".
- **Who decides `disabled`:** `Dropdown`'s `disabled` is `disabled.get().or(f_disabled)`. For a
  Select, `f_disabled` is always `None`: Select wraps `Dropdown` in its own `Field` and passes
  that Field no `disabled` (select.rs:231-238). `FieldContext.disabled` is a plain
  `Option<bool>`, not reactive.
- **The gallery's Select prop is static today.** `components.rs:899` reads
  `disabled: MaybeProp::from(p.bool("disabled"))`. `Props::bool` reads a `$cell` once and does
  not track it (props.rs `take` → `get_untracked`). Use `p.flag_signal("disabled").into()`;
  `MaybeProp<bool>` has `From<Signal<bool>>` (reactive_graph 0.2.16 `wrappers.rs:1901`). An
  absent prop becomes `Some(false)` instead of `None`. For a Select that changes nothing (see the
  previous fact).
- **The gallery's HTML elements take no event props today.** `render.rs::html` spreads
  `p.rest()` as attributes, and `rest()` warns about a `$fn` prop ("has no attribute form").
  The React gallery passes any `on…` prop through (`src/render.jsx` `value()`), so only the Rust
  side needs work.
- **The Rust Layer renders in place** (layer.rs:6), as tC4's does. The wrapper `div` is an
  ancestor of the list rows, so its capture listener sees the row click.
- **The rebuild's metadata lists a carried file with no role.** tc-store lists carried files
  with `role: None` (`crates/tc-store/src/materialize.rs:1005-1016`). `tc_burrito::build`
  writes `role` only when `Some` (`build.rs:251-253`). The planner's probe confirmed it: an OBS
  `MetadataSource` with `template_current_scope()` and `ingredients/360px/obs-en-01-01.jpg`
  (role `None`) builds a valid document whose entry has `checksum`, `mimeType: image/jpeg` and
  `size`, and no `role`.
- **tC4's own test layout** for a project picture is `ingredients/360px/<name>`
  (`test/obsResources.test.ts:97`). The template's first image line names `obs-en-01-01.jpg`.
- **Conflict copies have a different file name.** The other version of a carried file in
  conflict is `<stem>.conflict-<oid8>.<ext>` (STORAGE §3.1), and a case collision is
  `<stem>.case-<oid8>.<ext>`. Neither matches an image line's file name, so neither makes a
  picture ambiguous.
- **The ds-gallery screens were captured on another machine** (`/home/user`, 2026-10-16). On the
  owner's Linux PC, fallback glyphs (✓ ▾ ▴) can render differently (APP-DESIGN §7.3 found this
  on 57 of 62 app screens). Compare pixels only between captures made on this machine.

---

## 4. Steps

Start every shell command that runs cargo, rustc, node, npx or Playwright with `. ~/.tc-env && `.
Use `cargo -j 6`. Check `df -h /home` first. If it shows under 30 GB free and no rustc runs in
this worktree, run `python3 -I /home/bot/tc-rst/.audit/tools/prune-target.py
/home/bot/tc-rst/.claude/worktrees/app-fmisc/target`.

### Step 0. The plan

Commit this file with step 1.

### Step 1. tc-resources: Urdu (Devanagari) reads left to right (#701)

`crates/tc-resources/src/gateways.rs`:

1. Replace the ur-deva entry (lines 122-132) with:

   ```rust
   // tC4 lists ur-deva as right to left and names it اردو, in Arabic script. Devanagari reads
   // left to right, and the language's name in it is उर्दू (tC4 #701; docs/PLAN.md).
   Gateway {
       has_twl: false,
       has_simplified: false,
       ..gl(
           "ur-deva",
           "Door43-Catalog",
           "Urdu (Devanagari)",
           "उर्दू",
           Direction::Ltr,
       )
   },
   ```

2. In the module doc, after the flags list, add: "One entry differs from tC4 on purpose: ur-deva
   reads left to right and is named in Devanagari (tC4 #701)."
3. On the `direction` field (line 44), write the doc: "The direction of the language's script,
   which its helps read in. Where the installed helps' own manifest states a direction, a
   helps pane prefers it (`tc_helps::ResourceIdentity::direction`; unit B3, tC4 #701)."
4. Add two tests to `mod tests`. Helpers come first; they are test-only.

   ```rust
   /// A letter of a right-to-left script: Hebrew, Arabic, Syriac, Thaana, N'Ko and the
   /// supplements and presentation forms of these (Unicode bidi classes R and AL).
   fn is_rtl_letter(c: char) -> bool {
       matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF)
   }

   /// The BCP 47 script subtag of a tag (`deva` in `ur-deva`), when it names one.
   fn script_subtag(tag: &str) -> Option<&str> {
       tag.split('-').skip(1).find(|s| s.len() == 4 && s.chars().all(|c| c.is_ascii_alphabetic()))
   }

   /// Whether `c` belongs to `script`; `None` for a script this table does not know.
   fn in_script(script: &str, c: char) -> Option<bool> {
       let cp = c as u32;
       Some(match script.to_ascii_lowercase().as_str() {
           "deva" => matches!(cp, 0x0900..=0x097F | 0xA8E0..=0xA8FF),
           "arab" => matches!(cp, 0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF),
           "latn" => c.is_ascii_alphabetic() || matches!(cp, 0x00C0..=0x024F),
           _ => return None,
       })
   }

   #[test]
   fn ur_deva_helps_read_left_to_right_and_are_named_in_devanagari() {
       let ur = find("ur-deva", "Door43-Catalog").unwrap();
       assert_eq!(ur.direction, Direction::Ltr);
       assert_eq!(ur.autonym, "उर्दू");
       assert_eq!(display_name("ur-deva"), "उर्दू");
   }

   #[test]
   fn every_gateway_reads_in_the_direction_of_its_script() {
       for g in &GATEWAYS {
           let letters: Vec<char> = g.autonym.chars().filter(|c| c.is_alphabetic()).collect();
           assert!(!letters.is_empty(), "{}: no letters in {}", g.key(), g.autonym);
           let rtl = letters.iter().filter(|c| is_rtl_letter(**c)).count();
           assert!(rtl == 0 || rtl == letters.len(), "{}: {} mixes directions", g.key(), g.autonym);
           let script = if rtl > 0 { Direction::Rtl } else { Direction::Ltr };
           assert_eq!(g.direction, script, "{}: {} is written in a {script:?} script", g.key(), g.autonym);
           if let Some(tag) = script_subtag(g.language) {
               for c in &letters {
                   assert_eq!(in_script(tag, *c), Some(true), "{}: the tag names {tag}, the autonym {} has {c}", g.key(), g.autonym);
               }
           }
       }
       let rtl: Vec<String> = GATEWAYS.iter().filter(|g| g.direction == Direction::Rtl).map(|g| g.key().to_string()).collect();
       assert_eq!(rtl, ["fa::fa_gl", "ar::BSOJ"]);
   }
   ```

   `char::is_alphabetic` is true for Devanagari vowel signs such as U+0942 and false for the
   virama U+094D. Both cases are fine, because the script ranges contain the signs.
5. In `docs/PLAN.md`, add the #701 bullet (text in §8 below).
6. Mutation check (record it in the commit message): put back `"اردو", Direction::Rtl`. Both
   tests must fail, the second at the script subtag or at the direction. Then restore the fix.
7. Run `cargo test -p tc-resources -j 6` and `cargo clippy -p tc-resources --all-targets -j 6 -- -D warnings`.
   Commit: "tc-resources: Urdu (Devanagari) reads left to right and is named उर्दू (tC4 #701)".
   In the body, give the reason and say that it is a declared difference.

### Step 2. tc-obs: a project's own pictures are found by their path (#549)

`crates/tc-obs/src/images.rs`:

1. Module doc (lines 3-8). Say that the project's own pictures are found by their path in the
   ingredient table (`is_project_picture`), then the packs, then the default.
2. `ImageAsset.role` doc (line 72): "The burrito ingredient role. `resolve_image` counts a
   project asset only with `x-obsimages`, as tC4 does; `project_image_assets` gives that role to
   every picture it finds by path, so build project assets with it."
3. Add the following before `project_image_assets`:

   ```rust
   /// The folders below `ingredients/` that hold the app's own files and the stories, never a
   /// project's pictures (STORAGE §3.1, §3.2).
   const NOT_PICTURE_FOLDERS: [&str; 3] = ["checking", "tc", "content"];

   /// Whether the ingredient at `key` (as `metadata.json` lists it, `ingredients/…`) is one of
   /// the project's own pictures: a picture file below `ingredients/` whose first folder there is
   /// not `checking/`, `tc/` or `content/`. Its path decides, never its role: the rebuild's
   /// `metadata.json` lists a carried file with no role (STORAGE §3.3), and Pankosmia's rescan
   /// drops roles (tC4 BURRITO-SPEC S-2: paths are authoritative). tC4 takes a project picture
   /// only with the role `x-obsimages` (`projectImageIngredients`, `src/data/obsStory.ts:147-159`),
   /// so a picture a project carries never shows there (tC4 #549; a declared difference,
   /// docs/PLAN.md).
   pub fn is_project_picture(key: &str) -> bool {
       let Some(ipath) = key.strip_prefix("ingredients/") else {
           return false;
       };
       let first_folder = ipath.split_once('/').map_or("", |(first, _)| first);
       let name = ipath.rsplit('/').next().unwrap_or(ipath);
       !NOT_PICTURE_FOLDERS.contains(&first_folder) && is_picture_name(name)
   }
   ```

4. Replace `project_image_assets` (lines 168-185):

   ```rust
   /// The project's own pictures among the ingredient keys of its `metadata.json`
   /// ([`is_project_picture`]), each with the role [`resolve_image`] asks of a project asset.
   /// `uri_for` gets the path below `ingredients/`.
   pub fn project_image_assets<'a>(
       ingredient_keys: impl IntoIterator<Item = &'a str>,
       uri_for: impl Fn(&str) -> String,
   ) -> Vec<ImageAsset> {
       ingredient_keys
           .into_iter()
           .filter(|key| is_project_picture(key))
           .filter_map(|key| key.strip_prefix("ingredients/"))
           .map(|ipath| ImageAsset { uri: uri_for(ipath), role: Some(IMAGE_ROLE.to_owned()), decodable: None })
           .collect()
   }
   ```

   Leave `resolve_image` (lines 294-341) as it is (D2).
5. `crates/tc-obs/Cargo.toml` `[dev-dependencies]`: add
   `tc-burrito = { path = "../tc-burrito" }`, with the comment "# A project's own picture as the
   rebuild's metadata.json lists it: a carried file, with no role." This adds no build cost:
   tc-burrito is already in the dev graph through tc-resources → tc-journal → tc-burrito. **Do
   not** add tc-store: it depends on tc-obs.
6. New file `crates/tc-obs/tests/project_pictures.rs`:

   ```rust
   //! A story project's own pictures (tC4 #549): found by their path in `metadata.json`, which
   //! lists a carried file with no role.
   #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

   mod common;
   ```

   It uses `common::stories`, `tc_burrito::{IngredientEntry, MetadataSource, Md5, ProjectKind, build_metadata}`
   and `tc_obs::images::{…, is_project_picture, project_image_assets}`. It needs these helpers:
   - `fn uri_for(ipath: &str) -> String { format!("local://project/{ipath}") }`
   - `fn template_line() -> String`, the first `![` line of `stories("template")[0]`, as in
     `tc4_obs.rs:517-523`.
   - `fn metadata_keys(carried: &[&str]) -> (serde_json::Value, Vec<String>)`. It builds
     `MetadataSource { kind: ProjectKind::OpenBibleStories, format: (1, 0), fields: BTreeMap::new(), scope: tc_obs::template::template_current_scope(), relationships: vec![], nfc: true }`.
     The entries are `ingredients/content/01.md` (role `None`, a story),
     `ingredients/checking/settings.json` (role `Some("x-check-settings")`), and each carried path
     with role `None`, scope `None` and `md5: Md5::of(bytes)` of a few bytes. It calls
     `build_metadata(&source, &entries).unwrap()`, parses the bytes, and returns the document and
     its `ingredients` keys in document order.

   The tests:
   - **`a_project_picture_without_a_role_is_the_project_rung`**. Set
     `name = image_file_name(&template_line())` (`obs-en-01-01.jpg`) and
     `picture = format!("ingredients/360px/{name}")`. Call
     `metadata_keys(&[&picture, "ingredients/LICENSE.md"])`. Assert that
     `doc["ingredients"][&picture].get("role")` is `None` (the premise: the rebuild writes no
     role). Set `project = project_image_assets(keys…, uri_for)` and assert it has exactly one
     asset. Then build a primary pin's pack and the default pack, each holding `name`, and
     assert:
     `resolve_image(&line, &ImagePins { primary: Some(pin), fallback: None }, &project, &[pinned], &default) == ImageResolution::Project { uri: uri_for(&format!("360px/{name}")), file_name: name }`.
     The project's picture wins over both packs.
   - **`only_pictures_outside_checking_tc_and_content_are_the_projects`**. Table of
     `is_project_picture`. True: `ingredients/360px/obs-en-01-01.jpg`,
     `ingredients/obs-en-01-02.JPG`, `ingredients/pictures/content/obs-en-02-01.webp`,
     `ingredients/art/cover.png`, `ingredients/art/cover.jpeg`. False:
     `ingredients/checking/obs-en-01-01.jpg`, `ingredients/tc/obs-en-01-01.jpg`,
     `ingredients/content/obs-en-01-01.jpg`, `ingredients/content/front/title.jpg`,
     `ingredients/audio/01-01.mp3`, `ingredients/LICENSE.md`, `metadata.json`,
     `360px/obs-en-01-01.jpg`. Also assert that `project_image_assets` over the whole table returns
     exactly the five true keys' `uri_for(ipath)` values, in input order.
   - **`a_conflict_copy_keeps_the_picture_and_two_folders_make_it_ambiguous`**. With keys
     `ingredients/360px/obs-en-01-01.jpg` and `ingredients/360px/obs-en-01-01.conflict-1a2b3c4d.jpg`,
     the result is `Project` with the first's uri. With `ingredients/360px/obs-en-01-01.jpg` and
     `ingredients/images/obs-en-01-01.jpg`, it is not `Project`: it falls through to the default
     pack, by tC4's "two project matches" rule.
7. In `docs/PLAN.md`, add the #549 bullet.
8. Mutation check: temporarily make `project_image_assets` keep only role-tagged keys again (for
   example, filter everything out). The named test must fail at the `Project` assertion. Then
   restore. Confirm that `cargo test -p tc-obs -j 6` still passes
   `pictures_resolve_through_the_same_ladder_as_tc4` (the 26 vectors) and every `tc4_obs.rs`
   image test unchanged.
9. Run clippy for tc-obs with `--all-targets`. Commit: "tc-obs: a story project's own pictures
   are found by their path, not by a role (tC4 #549)". In the body, explain D2 and D3 and the
   S-2 rule.

### Step 3. tc-ui: a Select turned off while open closes and takes no choice (#552, code)

**Before you edit anything in tc-ui**, take the regression baseline of the Rust gallery on this
machine:

```sh
cd /home/bot/tc-rst/.claude/worktrees/app-fmisc/parity/ds-gallery
. ~/.tc-env && npm ci --offline --no-audit --no-fund
#   (if the offline install fails: cp -a /home/bot/tc-rst/.claude/worktrees/p0-a5/parity/ds-gallery/node_modules .)
. ~/.tc-env && CARGO_JOBS=6 node capture-rust.mjs       # builds the dev gallery, captures all 751 into screens-rust/
cp -a screens-rust <your scratchpad>/screens-rust-before
git checkout -- diff-report.json                        # see trap T1
```

`capture-rust.mjs` may exit 1 because of machine fonts. The screens are still written.

Then the code changes. They are small, because A5 and the app units also work in tc-ui.

1. `crates/tc-ui/src/ds/forms/select.rs`, in `choose` (lines 334-345). Make this the first
   statement:

   ```rust
   // A Select turned off while its list is open must not change (tC4 #552), even for a choice
   // that arrives before the list has closed.
   if disabled() {
       return;
   }
   ```

   Directly after `choose`, before `move_hi`, add:

   ```rust
   // A Select that turns off closes its list: nothing in it may be chosen (tC4 #552, where the
   // list stayed and took a row click). The focus is not sent back to the field, which is off.
   Effect::new(move |_| {
       if disabled() && open.get_untracked() {
           open.set(false);
       }
   });
   ```

   `disabled`, `open` and `choose` are `Copy` closures and signals, so nothing else changes. Keep
   both edits inside lines 300-460. A5's hunks in this file are at 198-215, 245-252, 531-540,
   652-660 and 672-712 (on `p0-a5`). Do not reformat the rest of the file.
2. `crates/tc-ui/src/gallery/components.rs:899`, in `select()`:

   ```rust
   // A screen turns a Select off while it is drawn (tC4 #552), so the prop follows its cell.
   disabled: p.flag_signal("disabled").into(),
   ```

3. `crates/tc-ui/src/gallery/render.rs::html` (lines 98-149). Read the handler before
   `p.rest()`, and attach it only on the generic-element arm:

   ```rust
   // The capture phase runs before the component under test sees the click, so a case can
   // turn a control off with the very click that reaches it (tC4 #552).
   let on_click_capture = p.handler::<leptos::ev::MouseEvent>("onClickCapture");
   let mut attrs = p.rest();
   …
   other => {
       … (the tag check stays)
       let el = element(other.to_string()).add_any_attr(spread(&attrs));
       match on_click_capture {
           Some(cb) => el.on(leptos::ev::capture(leptos::ev::click), move |e| cb.run(e)).child(kids).into_any(),
           None => el.child(kids).into_any(),
       }
   }
   ```

   Add a line to the module doc's node list: "`onClickCapture` on an HTML element: a
   `{ "$fn": … }` called in the click's capture phase." `action.rs:181-213` shows that
   `.add_any_attr(…).on(ev::capture(…), …)` compiles on `element(…)`.
4. Build and compare:

   ```sh
   . ~/.tc-env && CARGO_JOBS=6 node capture-rust.mjs
   diff -r <your scratchpad>/screens-rust-before screens-rust && echo "751 identical"
   git checkout -- diff-report.json
   ```

   Every PNG must be byte-identical: the changes draw nothing in any existing case.
5. Mutation checks. They need the step 4 cases, so do them after step 4 and record them in the
   step 4 commit message.
6. Run wasm clippy for tc-ui: no features, `--features gallery` and `--features app`. Also run
   `cargo test -p tc-ui -j 6`. Commit: "tc-ui: a Select that turns disabled while its list is
   open closes it and takes no choice (tC4 #552)". Include the 751-identical result.

### Step 4. The gallery cases and the declared-difference gate (#552, proof)

1. `parity/ds-gallery/cases/forms.mjs`. Next to the `select` helper (line 39), add:

   ```js
   /* tC4 #552: the click that chooses a row also turns the Select off, in its capture phase,
      as a screen that turns a form busy at that moment would. tC4 still takes the choice; the
      port closes the list and keeps the value (residuals.json). The caption shows what onChange
      handed over. */
   const offOnChoose = (n) => el('div', {
     onClickCapture: setTo('off', true),
     style: { display: 'flex', flexDirection: 'column', gap: 8 },
     children: [n, el('Text', { role: 'caption', children: ['Chosen: ', cell('picked')] })],
   });
   const offWhileOpen = select({ label: 'Book', options: bookOptions(false), value: 'RUT', searchPlaceholder: 'Search books', disabled: cell('off'), onChange: set('picked', '0.target.value') });
   const OFF_WHILE_OPEN = [focus('[role="combobox"]'), key('ArrowDown'), click('[role="option"]:nth-child(2)')];
   ```

   At the end of the `Select` group, add:

   ```js
   ['disabled-while-open', offWhileOpen, { frame: { height: 500 }, cells: { off: false, picked: 'none' }, wrap: offOnChoose, steps: OFF_WHILE_OPEN,
     note: 'tC4 #552: the Select turns off as a row is clicked in its open list. tC4 takes the choice; the port closes the list and takes none (residuals.json).' }],
   ['disabled-while-open-label', offWhileOpen, { frame: { height: 500 }, cells: { off: false, picked: 'none' }, wrap: offOnChoose, steps: [...OFF_WHILE_OPEN, click('label')],
     note: 'The same, then a press on the label of the field that is now off: the list stays closed and nothing is chosen.' }],
   ```

   Use `value: 'RUT'` (the first row), so the list does not scroll to the highlighted row. Twelve
   books make the list searchable, so the focus is in the search field when the field turns off.
   The label step is the spec's "label-press edge". Do not use a click on the field itself:
   Playwright waits 30 s for a disabled button and fails (T10).
2. Capture both sides on this machine:

   ```sh
   cd parity/ds-gallery
   . ~/.tc-env && TC4_ROOT=/home/bot/unfoldingword/translationcore4 node capture.mjs --only Select/disabled-while-open
   #   (writes gallery-spec.json, builds dist/, and writes tC4's two screens; --only is a prefix, so both cases)
   . ~/.tc-env && CARGO_JOBS=6 node capture-rust.mjs --only Select/disabled-while-open   # exits 1 until residuals.json exists
   . ~/.tc-env && node dom-diff.mjs --only Select/disabled-while-open --show 50
   ```

   Check that tC4's screen shows "Chosen: EST" with the list closed and the field disabled, and
   the port's shows "Chosen: none" with the same. `diffs/Select/<case>.png` must mark only the
   caption. The DOM lines printed must be exactly one pair, tC4 `"Chosen: EST"` against the port
   `"Chosen: none"`, the same for both cases. If anything else differs (a `.focused` marker, a
   listbox, a style), stop and find out why before you declare it. A difference other than the
   caption means the case or the port is wrong. The one exception is a focus marker that Chromium
   leaves on tC4's disabled button. If that happens, declare it with the reason.
3. `parity/ds-gallery/residuals.json` (new). One entry per case:

   ```json
   {
     "Select/disabled-while-open": {
       "ruling": "#552",
       "layers": ["pixels", "dom"],
       "why": "tC4 #552 (still open at 9ad4d5c): a Select made disabled while its list is open keeps the list, and a row click still calls onChange, so tC4's caption reads EST. The port closes the list when the Select turns disabled and takes no choice while it is disabled, so its caption reads none. Declared difference (AGENTS.md, Matching tC4; docs/PLAN.md).",
       "dom": [{ "tc4": "\"Chosen: EST\"", "port": "\"Chosen: none\"" }]
     },
     "Select/disabled-while-open-label": { "…": "the same, with the label sentence" }
   }
   ```

   Lines are compared trimmed. Write the pairs you actually observed.
4. `parity/ds-gallery/diff.mjs`. The explanation may now be an object: replace
   `why: residuals[c.id]` with `why: typeof residuals[c.id] === 'string' ? residuals[c.id] : residuals[c.id].why`.
   Update the header comment.
5. `parity/ds-gallery/dom-diff.mjs`. Read `residuals.json` as `diff.mjs` does. Add
   `allDiffs(a, b)` → `[{ line, tc4: a[i] ?? null, port: b[i] ?? null }]` for **every** index
   where the lines differ. For a row that differs and has a residual, add
   `why: residual.why ?? residual` and `lines: allDiffs(…)`. Keep `diffs` (the first `show`
   strings) for printing. Update the header comment.
6. Rerun with the residuals in place, then run the full DOM and behaviour comparisons:

   ```sh
   . ~/.tc-env && node diff.mjs --only Select/disabled-while-open        # adds `why` to both rows
   . ~/.tc-env && node dom-diff.mjs --only Select/disabled-while-open    # adds `why` and `lines`
   . ~/.tc-env && node dom-diff.mjs                                      # all 753: expect 751 same and the 2 declared
   . ~/.tc-env && node scenarios.mjs                                     # expect 53 of 53 same, 277 steps
   ```

   **Never** run `diff.mjs`, `capture-rust.mjs` or `capture.mjs` without `--only` here (T1,
   T20). `dom-diff.mjs` and `scenarios.mjs` compare the DOM, focus and node identity. These do
   not depend on the machine, so a full run is right. `scenario-report.json` should come out
   byte-identical. If it does not, find out why.
7. `crates/tc-ui/tests/gallery_record.rs`. Keep the existing tests. Make additive edits only:
   - Change the module doc to say that a case may differ from tC4 only as `residuals.json`
     declares.
   - Add `fn residuals() -> BTreeMap<String, Value>`. An absent file gives an empty map. A file
     that is not an object panics.
   - Add `fn declares(residual: Option<&Value>, layer: &str) -> bool`.
   - Add `fn plan_differences() -> String`. It returns the lines of `../../docs/PLAN.md` after
     `### Where the rebuild differs from tC4 4.0 on purpose`, up to the next line that starts
     with `#`. This is the same section `screen_record/gate.rs::plan_rulings` reads.
   - Add the pure
     `fn residual_problems(id: &str, residual: &Value, pixels: &Value, dom: &Value, plan: &str) -> Vec<String>`.
     It reports each of the following:
     - `why` missing or blank;
     - `ruling` missing, or `plan` not containing `({ruling})`;
     - `layers` empty, repeated, or holding something other than `pixels` and `dom`;
     - `pixels` declared while `mismatched` is not a number above 0, or not declared while it
       is not `Some(0)`;
     - a non-empty `errors` in the DOM row;
     - `dom` declared while `same != Some(false)`, or while the trimmed `lines` pairs differ from
       the residual's `dom` pairs (a missing side reads `(none)`);
     - `dom` not declared while `same != Some(true)`.
   - In `every_case_has_tc4s_screen_and_was_compared_clean`, skip in `off` the ids whose residual
     declares `pixels`, and in `differ` those that declare `dom`. Nothing else changes there.
   - New `#[test] every_declared_difference_names_exactly_what_differs`. Every residual id is a
     spec case, and `residual_problems` is empty for each.
   - New `const ISSUE_CASES: &[(&str, &str)] = &[("Select/disabled-while-open", "en"), ("Select/disabled-while-open-label", "en")];`
     with `#[test] the_gallery_draws_every_state_a_tc4_issue_asks_for`, written like the DELTA
     test. Each case must also have a residual.
   - New `#[test] a_declared_difference_that_is_gone_or_wider_fails`. Use planted JSON values: a
     good residual (`ruling "#552"`, both layers, one pair; pixel row `mismatched: 40`; DOM row
     `same: false`, `errors: []`, `lines` with that pair, indented; plan `"- x (#552)"`) gives no
     problem. Each of these gives at least one problem: the pixels at 0 with the DOM the same (the
     difference is gone); an extra line pair (the difference is wider); a plan without `(#552)`;
     an empty `why`.
8. `parity/ds-gallery/README.md`:
   - Count from the regenerated spec with a one-liner, for example
     `node -e "const s=require('./gallery-spec.json');…"`. Expect 753 cases, 716 component cases,
     Select 21, `steps` 116, `render` 73, `cells` 21. Recount `needs` rather than guessing.
   - Replace the `residuals.json` row in "Layout": "A declared difference per case id: the
     ruling docs/PLAN.md lists, the layers that differ (`pixels`, `dom`), why, and the exact
     [tC4, port] DOM lines. `cargo test -p tc-ui` fails when a declared case no longer differs,
     differs in more than it declares, or cites a ruling PLAN lacks."
   - Update "Result at this commit": 751 cases at 0 mismatched pixels and the same DOM, plus 2
     declared (#552). 53 of 53 scenarios.
   - Add one sentence: both sides of the two #552 cases were captured on the owner's Linux PC
     (Chromium 141, build 1194), the rest on the machine named above.
   - Note that `gallery-spec.json`'s `source.repo` now names `/home/bot/…`, because `TC4_ROOT`
     pointed there (T2).
9. `docs/PLAN.md`: add the #552 bullet. It must contain `(#552)`.
10. Mutation checks for step 3, recorded in this commit's message. For each one, rebuild with
    `CARGO_JOBS=6 node capture-rust.mjs --only Select/disabled-while-open` and run
    `node dom-diff.mjs --only Select/disabled-while-open` and the gallery_record test, then
    restore:
    - (a) Remove the close `Effect`, keep the guard: the port leaves the list open with "Chosen:
      none". The residual test fails, because the DOM differs in more lines than declared. This
      proves the guard, which no browser case can reach once the effect exists (§3).
    - (b) Remove the guard, keep the effect: the cases are unchanged (expected; §3).
    - (c) Remove both: the port shows "Chosen: EST", like tC4. The residual test fails, because
      the difference is gone.

    After restoring, rerun the two `--only` captures and dom-diff so that the committed reports
    are the fixed port's. Check `git diff --stat` before you commit.
11. Commit what changed: `parity/ds-gallery/{cases/forms.mjs, gallery-spec.json, screens/Select/disabled-while-open.png, screens/Select/disabled-while-open-label.png, residuals.json, diff.mjs, dom-diff.mjs, diff-report.json, dom-report.json, README.md}`,
    and `scenario-report.json` only if it changed and you understand why. Also
    `crates/tc-ui/tests/gallery_record.rs` and `docs/PLAN.md`. Never commit `node_modules/`,
    `dist/`, `diffs/` or `screens-rust/`; they are git-ignored, so check `git status`. Commit:
    "Gallery: tC4 #552 as two declared differences, checked line by line".

### Step 5. Last checks and the last commit

Run the commands in §7. Fix anything they report and commit. If nothing changed, there is no
commit.

---

## 5. Invariants (each is held by a test above)

- **No translator's work is lost, hidden or silently rewritten.** These changes write nothing.
  The Select change can only refuse a change while the field is off, which is what "disabled"
  promises. A form that turns busy, such as Feedback's `disabled=busy` or App language's
  `disabled=locked` on p0-a5, can no longer change its value behind that state. The picture rule
  only shows more of the project's own pictures: a carried picture that tC4 hid now shows. No
  image line is rewritten (`resolve_image` reads only). The registry change alters only a label
  and a direction.
- **The screens match tC4 4.0 except where declared.** All 751 existing gallery cases keep their
  pixels (step 3.4, on this machine), their DOM (full dom-diff) and their behaviour (53
  scenarios). The two new cases differ only in the declared lines.
- **tC4's resolution rule is unchanged** (the 26 vectors). Only which ingredients count as the
  project's pictures changes.
- **Project picture selection depends only on the path.** It never depends on a role, the
  listing order or a conflict copy (the three tc-obs tests).
- **Direction follows the script** for all 18 registry entries. Only `fa::fa_gl` and `ar::BSOJ`
  read right to left.

---

## 6. Tests that prove "done" (names and what each asserts)

| Test | Where | Asserts |
|---|---|---|
| `ur_deva_helps_read_left_to_right_and_are_named_in_devanagari` | tc-resources `gateways.rs` tests | ur-deva is `Ltr`, its autonym is उर्दू, and `display_name("ur-deva")` is उर्दू |
| `every_gateway_reads_in_the_direction_of_its_script` | same | for every entry, the autonym's letters are in one direction and that direction is the entry's; a tag's script subtag holds every letter; the right-to-left set is exactly fa and ar |
| `a_project_picture_without_a_role_is_the_project_rung` | `crates/tc-obs/tests/project_pictures.rs` | `build_metadata` lists the carried picture with no role; it is found and wins over a pinned pack and the default |
| `only_pictures_outside_checking_tc_and_content_are_the_projects` | same | the path rule, table by table, and the assets in input order |
| `a_conflict_copy_keeps_the_picture_and_two_folders_make_it_ambiguous` | same | a `.conflict-<oid8>` copy does not make a picture ambiguous; two folders with one name fall through |
| `pictures_resolve_through_the_same_ladder_as_tc4` and the `tc4_obs.rs` image tests | existing | still pass: tC4's resolver is unchanged |
| ds-gallery `Select/disabled-while-open` | `parity/ds-gallery` with `residuals.json` | tC4 takes the choice; the port closes the list and takes none |
| ds-gallery `Select/disabled-while-open-label` | same | as above, then a label press neither opens the list nor chooses |
| `every_declared_difference_names_exactly_what_differs` | `crates/tc-ui/tests/gallery_record.rs` | each residual differs exactly as declared, and PLAN lists its ruling |
| `the_gallery_draws_every_state_a_tc4_issue_asks_for` | same | both #552 cases exist, in English, with a residual |
| `a_declared_difference_that_is_gone_or_wider_fails` | same | the residual check catches a difference that is gone, a wider one, a ruling PLAN lacks and an empty `why` |
| `every_case_has_tc4s_screen_and_was_compared_clean` | same (edited) | the other 751 cases are still at 0 and the same |

---

## 7. Checks before the last commit (all must pass)

```sh
cd /home/bot/tc-rst/.claude/worktrees/app-fmisc
. ~/.tc-env && cargo fmt --all -- --check
. ~/.tc-env && cargo clippy --workspace --all-targets -j 6 -- -D warnings
. ~/.tc-env && cargo test --workspace -j 6
. ~/.tc-env && cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -j 6 -- -D warnings
. ~/.tc-env && cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -j 6 --features gallery -- -D warnings
. ~/.tc-env && cargo clippy -p tc-ui --target wasm32-unknown-unknown --all-targets -j 6 --features app -- -D warnings
#   (harness does not exist on tc4fix; if a merge brings it, add --features harness)
. ~/.tc-env && cargo clippy -p tc-api --target wasm32-unknown-unknown --all-targets -j 6 -- -D warnings
```

## 8. Text for docs/PLAN.md

Insert the following after the bullet "- Every other question in that §5 follows its
recommendation." (line 64) and before "The rebuild does not port tC4's sync mechanism" (line 66),
so that it stays inside the section that `screen_record` and `gallery_record` read. Add each
bullet in the commit of its step:

```markdown
These differences follow the owner's rule in AGENTS.md ("Matching tC4", 2026-10-17), and the
owner is told of each:

- Urdu in Devanagari script (`ur-deva`) reads left to right and is named उर्दू (#701). tC4 4.0's
  registry says right to left and names it اردو in Arabic script, so its helps would read
  backwards. The Source texts screen draws that row in Devanagari; B9a masks it and cites this
  entry.
- A story project's own pictures are found by their path below `ingredients/`, outside
  `checking/`, `tc/` and `content/` (#549). tC4 4.0 takes only an ingredient with the role
  `x-obsimages`, which the rebuild's `metadata.json` does not write for a carried file and
  Pankosmia's rescan drops, so a project's own pictures would never show.
- A Select that turns disabled while its list is open closes the list, and a choice made then
  changes nothing (#552). tC4 4.0 keeps the list open, and a row click still changes the value.
  The gallery's two `Select/disabled-while-open` cases differ from tC4 on purpose
  (`parity/ds-gallery/residuals.json`).
```

## 9. Traps

- **T1 Machine fonts.** The committed `screens/` came from the cloud machine. `capture-rust.mjs`
  always runs `diff()`. A full run here rewrites `diff-report.json` with font mismatches. After
  every full `capture-rust.mjs`, run `git checkout -- parity/ds-gallery/diff-report.json`. Use
  `--only` for anything you commit.
- **T2 `TC4_ROOT`.** `build-spec.mjs` and `vite.config.mjs` default to
  `/home/user/unfoldingword/translationcore4`, which does not exist here. Set
  `TC4_ROOT=/home/bot/unfoldingword/translationcore4`. The spec's `source.repo` then changes.
  That is the only change to existing spec content. The tC4 checkout has no `node_modules`; the
  gallery's own `node_modules` and Vite's `dedupe` cover React.
- **T3 Node modules.** Use `npm ci --offline` in this worktree's `parity/ds-gallery`, or copy
  p0-a5's `node_modules` (reading another worktree is allowed; never change it). Playwright
  1.56.1 uses Chromium build 1194 in `~/.cache/ms-playwright`, with the override in
  `~/.tc-env`.
- **T4 Build jobs.** `build-gallery.sh` uses `cargo -j ${CARGO_JOBS:-2}`. Set `CARGO_JOBS=6`.
  The script copies `gallery-spec.json` into `target/tc-ui-gallery/` at build time, so rebuild
  (or run `capture-rust.mjs` without `--no-build`) after the spec changes.
- **T5 The guard is unreachable from a browser** once the close effect exists (§3: native
  listeners, removal during dispatch, effects at the microtask checkpoint). Prove it with
  mutation (a) in step 4.10. Do not try to write a case for it.
- **T6 Conflict with A5 in `components.rs`.** A5 (branch `p0-a5`) changes the lines around
  `select()`'s `disabled:` line (`label`, `hint`, `search_placeholder`, `no_matches_label`,
  `clear_label` → `Signal::from`). Git reports adjacent changes as a conflict, so the merge of
  `tc4fix` with `app` (once A5 is in) will conflict on these lines. Keep A5's lines and this
  `disabled:` line. Say so in the step 3 commit message. `select.rs` does not overlap (step
  3.1).
- **T7 Do not call `close()` from the disable effect** (D5).
- **T8 Do not read a cell with `Props::bool`** for a prop that must follow the cell; it reads
  once.
- **T9 `dom-diff.mjs` normally keeps only the first 3 differing lines.** The residual check needs
  `lines` with all of them for declared cases (step 4.5).
- **T10 Playwright and disabled controls.** A Playwright click waits for a disabled control to
  become enabled and times out after 30 s. That is why the label is pressed, never the disabled
  field.
- **T11 `value: 'RUT'`.** With `TIT`, the open list scrolls to Titus, and Playwright scrolls
  again to reach row 2. The case would still work, but less plainly.
- **T12 tc-obs dev-dependencies.** tc-burrito is safe; tc-store would form a cycle (tc-store
  depends on tc-obs).
- **T13 Keep `resolve_image`'s role check** (D2). Removing it would break the "project without
  role" vector, which pins tC4's resolver.
- **T14 PLAN placement.** Put the bullets inside the "Where the rebuild differs…" section,
  before `## Definition of done`, with `(#552)` written exactly. `screen_record`'s
  `plan_rulings` parses only `(Qn)` and ignores these bullets.
- **T15 Sibling fix units** (`app-ftext`, `app-fhelps`) also branch from `tc4fix` and may edit
  `docs/PLAN.md`'s list. A conflict there at merge is expected and easy to resolve. Keep each
  bullet self-contained.
- **T16 Do not edit TC4-ISSUES.md or TC4-DELTA** (§1).
- **T17 The Source texts screen** shows ur-deva (`parity/tc4-baseline/dom/source-texts.json`).
  It is B9a's screen. The PLAN bullet tells B9a to mask the row.
- **T18 Never run `capture.mjs` without `--only`.** A full run recaptures and rewrites all 751
  tC4 PNGs on this machine and prunes screens.
- **T19 The tC4 checkout is read-only.** Never write there. `capture.mjs` only reads it.
- **T20 Do not run `diff.mjs` without `--only`** either (T1).
- **T21 Leftovers from mutation checks.** Rebuild and recapture after restoring (step 4.10). The
  committed reports must come from the fixed port. Check `git diff` of the reports before you
  commit.

## 10. What later units rely on (keep these names)

- `tc_obs::images::project_image_assets(ingredient_keys, uri_for)` and
  `tc_obs::images::is_project_picture(key)`. B7 builds the project rung from `metadata.json`'s
  keys. The `/media` route can use `is_project_picture` to decide whether it serves a requested
  project file as a picture.
- `tc_resources::gateways::GATEWAYS` ur-deva: `Direction::Ltr`, autonym उर्दू. B3 shows it and
  may prefer `ResourceIdentity::direction` from the installed manifest. B9a masks the
  `source-texts` row.
- `Select`: a `disabled` that turns true closes the list and refuses choices. A5's Feedback and
  App language dialogs get this behaviour.
- `parity/ds-gallery/residuals.json` and the `gallery_record` residual check: the place for any
  later design-system difference from tC4. It needs a ruling that PLAN lists.

## 11. For the final answer

- Items handled: TC4-ISSUES §1 row 12 / #701 (both tests), row 13 / #552 (the two gallery cases,
  the residual gate, the label edge), and row 15 / #549 (the named test and two more), each with
  its PLAN bullet.
- Not done, with reasons: B3's manifest direction preference (B3 is not built); the
  `source-texts` mask (B9a); the Devanagari face (#613, A5); the TC4-ISSUES status updates
  (shared tables; integrator).
- The owner must be told of three declared differences: #701, #549 and #552. #552 is not yet in
  DELTA-10-19 §5's "Differences to tell the owner".
- Report the measurements: 751 identical Rust screens before and after, the dom-diff counts, the
  scenarios count, and the mutation check results.
