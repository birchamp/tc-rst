# translationCore 4 — UI and user-visible behavior inventory

Lane: user interface, interaction and visual design, for a Rust rebuild at full parity.
Source: `/home/user/unfoldingword/translationcore4` at commit `d494a9d` (2026-10-14), package `uw-tc4` version `4.0.0-rc.1` (`package.json:2-3`).
All paths below are relative to that repository root. `file:line` citations point at the current tree.

Read-only research. Other lanes cover the data layer (journal, store, burrito format), docs, and Door43 sync.
Where the UI depends on a data-layer action, this report names the action in `src/state.jsx` and stops there.

---

## 0. Orientation

### 0.1 The shape of the UI

- One window. A 56 px Ocean title bar on top (`AppHeader`), one main view below it, and modals/drawers/popovers layered over both.
- No router. The current screen is one string in app state, `s.view`: `home | read | draft | check | publish` (`src/state.jsx:372`).
  - `read` is labelled **Understand**, `draft` is labelled **Translate**, `check` is **Check**, `publish` is **Community Checking** (a sub-screen of Check, no tab of its own) (`src/state.jsx:172` `MODE_NAME`).
- Modals are one string too, `s.modal`: `null | addProject | newProject | newObs | addBook | settings | sources | fix | import | signIn | share | feedback` (`src/state.jsx:438`). Some dialogs are not in `s.modal`: gateway change (`s.gatewayPreview`), upgrade confirm (`s.upgrade.preview`), internet ask (`s.netAsk`), net failed (`s.netFailed`), and the account-menu dialogs (local React state in `AccountMenu.jsx`).
- Two project kinds with parallel screens: a **Bible project** (`flavor: textTranslation`; book → chapter → verse) and an **OBS project** (`flavor: textStories`; story → frame) (`src/App.jsx:181-198`).
- All app state lives in one reducer + context, `AppProvider` / `useApp()` in `src/state.jsx` (7086 lines). Views are thin.
- The whole window is `100vh`, flex column, `overflow: hidden`; each view scrolls its own panes (`src/App.jsx:123`).

### 0.2 Screen list (Bible project unless noted)

| # | Screen | Component | Reached from |
|---|---|---|---|
| S1 | Home (project list) | `src/views/Home.jsx` | launch; brand click; project chip; "Back to Home" |
| S2 | Understand | `src/views/Understand.jsx` | mode switch "Understand"; default open view (D87) |
| S3 | Translate | `src/views/Draft.jsx` (+ `SectionEditor.jsx`) | mode switch "Translate"; resume |
| S4 | Conflict review (inside Translate) | `src/views/ConflictReview.jsx` | Home conflicts chip; Translate toolbar button; verse marker |
| S5 | Check — tool picker | `src/views/Check.jsx:1343-1448` | mode switch "Check" |
| S6 | Check — tW/tN session | `src/views/Check.jsx:1196-1294` | tool card |
| S7 | Check — Align workspace | `src/views/Check.jsx:1109-1130` + `src/views/Align.jsx` | Align card |
| S8 | Community Checking (preview + export) | `src/views/CommunityChecking.jsx` | Community Checking card |
| S9 | OBS Understand | `src/views/StoryUnderstand.jsx` | mode switch, OBS project |
| S10 | OBS Translate | `src/views/StoryDraft.jsx` | mode switch, OBS project |
| S11 | OBS Check picker / session | `src/views/Check.jsx` (story mode) | mode switch, OBS project |
| S12 | OBS Community Checking | `src/views/CommunityChecking.jsx:96-157` | card |
| S13 | Help and guides (full-window panel with iframe) | `src/views/AccountMenu.jsx:186-240` | account menu |
| — | View error fallback | `src/App.jsx:152-179` | any render crash |
| — | Open-progress overlay | `src/views/OpenProgress.jsx` | slow project open (>300 ms) |
| — | Dev Inspector | `src/views/dev/Inspector.jsx` | Alt+Shift+I, dev builds only |

### 0.3 Modal and dialog list

| Dialog | Component | Trigger |
|---|---|---|
| Add a project (chooser) | `src/views/modals/AddProject.jsx` | Home "+ Add a project" |
| New Bible | `src/views/modals/NewBible.jsx` | chooser card |
| New Open Bible Stories | `src/views/modals/NewObs.jsx` | chooser card |
| Import a project (3 steps + working + failed) | `src/views/modals/Import.jsx` | chooser card |
| Add a book (method → pick / USFM) | `src/views/modals/AddBook.jsx` | Home "Add a book" tile; automatically after New Bible |
| Project settings | `src/views/modals/ProjectSettings.jsx` | Home card "Settings" |
| Get source texts (+ "Pinned resources" updates) | `src/views/modals/SourceTexts.jsx`, `ResourceUpdates.jsx` | Settings "Manage source texts"; Check "Get source texts"/"Download it"; OBS "Get source" |
| Upgrade confirmation (set / text / re-pin) | `src/views/modals/UpgradeSet.jsx` | Upgrade buttons; guided fix re-pin |
| Change checking language? | `src/views/modals/GatewayChange.jsx` | Settings package row; Sources "Use for checking" |
| Guided fix (missing pinned resource) | `src/views/modals/GuidedFix.jsx` | Check tool card "Fix this…" |
| Sign in to Door43 | `src/views/modals/ShareSignIn.jsx` | Share; account menu; "Change" |
| Share on Door43 / Upload changes | `src/views/modals/ShareDialog.jsx` | Home card Share / Upload changes |
| Feedback | `src/views/modals/Feedback.jsx` | account menu "Report a problem"; banner "Ask for help" |
| Turn on the internet? / tC4 could not use the internet | `src/views/AccountMenu.jsx:356-379` | any internet step while off |
| App language | `src/views/AccountMenu.jsx:302-350` | account menu |
| About translationCore → License | `src/views/AccountMenu.jsx:247-289` | account menu |
| Remove all the text of section …? | `src/views/SectionEditor.jsx:244-255` | Save section with no words |
| Translation Academy drawer | `src/views/Check.jsx:339-349` | Check "Learn more: … →" |

---

## 1. App shell

### 1.1 Entry

- `src/main.jsx:13` wraps `window.fetch` with `guardFetch(…, internet, origin)`: while the internet switch is off, a request to another origin (or a platform route that uses the internet) is refused before it is sent. This is user-visible behavior: nothing leaves the computer unless the user turned the internet on.
- `src/main.jsx:15-21` renders `<AppProvider><App/><DevAnnotate/></AppProvider>` in StrictMode. `DevAnnotate` loads only under `vite --mode annotate` (`src/DevAnnotate.jsx`).
- `index.html`: `<html lang="en">`, title "translationCore 4", favicon `/assets/logo-mark-color.png`. The i18n module sets `lang` and `dir` on `<html>` at load (`src/i18n/index.js:33-40`).
- Global CSS: `src/ds/styles.css` (token imports) then `src/ui.css` (30 lines of app leftovers).

### 1.2 Top bar (`TopBar`, `src/App.jsx:85-118`; `AppHeader`, `src/ds/components/navigation/AppHeader.jsx`)

- 56 px tall (`--header-height`), Ocean fill (`#014263`), dark-ground context (`data-on="dark"`), 16 px side padding.
- Three-column grid `minmax(0,1fr) auto minmax(0,1fr)`: the center column stays centered whatever the sides hold (#599; `AppHeader.jsx:25`). J16 asserts the switch center is within 1 px of the window center at 1024 px width, with the longest save status (`e2e/j16-understand.spec.ts`).
- Left column:
  - Brand: `assets/translationcore-logo.png` (28 px high) + wordmark "translationCore" with a superscript "®" (literal English, not i18n). Click → `actions.backToProjects` (`src/App.jsx:97`).
  - Vertical rule.
  - Project chip (only with a project): 26 px square initials badge (first letters of the first two words of the name, uppercase), project name (ellipsis, `dir` = project direction), language tag beneath. Tooltip "{name}. Switch project". Click → `backToProjects` (`AppHeader.jsx:37-52`; `src/App.jsx:92-98`).
- Center column (only when a project is open and the view is not Home): a pill `Switcher` with three tabs — **Understand** (`read`), **Translate** (`draft`), **Check** (`check`). On `publish` (Community Checking) the Check tab stays selected (`src/App.jsx:103-108`). `data-testid="mode-switch"`.
- Right column: the **save indicator** and the **account menu** button.

### 1.3 Save indicator (`SaveIndicator`, `src/App.jsx:41-83`)

- Shown only while a project is open. A status dot + label. Color on the dark bar: `rgba(255,255,255,.66)`, error `--tc-invalid-on-dark` (#F2938A).
- States and labels: `saved` "Saved" (valid dot), `saving` "Saving…" (warn), `dirty` "Unsaved changes" (warn), `error` "Save failed — retry" (invalid) + a **Retry** button.
- The effective state is the worst of five save schedulers: verse text, user comments (notes), alignments, check decisions, OBS story units. Rank: error > saving > dirty > saved (`src/App.jsx:56-61`).
- A failed checkpoint commit (and no save error) shows "Checkpoint not saved" with its own Retry (`src/App.jsx:65-68`).
- Retry targets (`retryFor`, `src/App.jsx:32-37`): story save → `retry-story-save`; note save → `retry-note-save`; checkpoint → `retry-checkpoint`; else the verse save.
- The indicator binds to the real write promise, never optimistic (FR-32). The verse scheduler's idle debounce is **2000 ms** (`src/data/saveScheduler.ts:93`); blur flushes at once.

### 1.4 Account menu (`src/views/AccountMenu.jsx`)

- Trigger: a 34 px circle at the right of the top bar (`AccountMenu.jsx:54-70`).
  - Signed out / sign-in saved: person glyph on translucent white, 1.5 px white border.
  - Signed in: Inspire fill, white 2 px border, the first two letters of the login, uppercase, weight 900.
  - Internet on: an 11 px Inspire dot with a white border at the top-end corner (`data-testid="internet-indicator"`), tooltip "The internet is on. Click to turn it off." Accessible name: "Door43 account: @{user}" / "…: sign-in saved" / "…: not signed in", plus the tooltip when on.
  - `aria-haspopup="menu"`, `aria-expanded`; a 3 px Inspire ring while open.
- Panel: an anchored popover (`Layer level="popover" placement="anchor" align="end"`), `role="menu"`, card surface with radius lg, dismiss on outside press and Escape (`AccountMenu.jsx:122-155`). Rows are two-line (title + one-line explanation). Order:
  1. Signed in: "@{user}" / "Open my page on Door43" → opens the profile in the system browser (an internet step, kind `profile`).
  2. Sign-in saved: "Sign-in saved on this computer" / "Check which Door43 account is saved" → `checkSavedSignIn` (internet step `checkSignIn`).
  3. Signed out: "Sign in to Door43" / "Sign in to share projects on Door43." → sign-in dialog.
  4. Account error line (role alert), e.g. "Door43 did not accept the saved sign-in, so it was removed. Sign in again."
  5. Rule.
  6. "Use the internet" (`role="menuitemcheckbox"`, `aria-checked`) with a 32×18 switch; sub-line "On until you turn it off or close tC4." / "Off. A step that needs the internet asks first." Clicking does not close the menu.
  7. "Help and guides" → Help panel (S13).
  8. "App language" / current language's native name → App language dialog.
  9. "Report a problem" → Feedback dialog (General Feedback, fresh or the kept unsent report).
  10. "About translationCore" → About dialog.
  11. (signed in or saved) Rule + "Sign out of Door43" (local only, no request).
- Keyboard contract (`AccountMenu.jsx:91-116`): on open the first row takes focus (held for up to 10 animation frames); ArrowDown/ArrowUp cycle, Home/End jump, Tab closes the menu; Escape closes and focus returns to the trigger (Layer).
- Opening the menu sends no request (D88).

### 1.5 View error boundary (`src/App.jsx:148-179`)

- Each view is wrapped in a boundary keyed by `view:repoPath`, so a navigation clears it.
- On a render error the top bar stays (the save indicator keeps its Retry). The body shows a warn Callout (role alert, `data-testid="view-error"`): "**This screen could not be shown.** {message}" + button "Back to Home".

### 1.6 Window chrome (desktop)

- `scripts/preload.cjs:28-31` sets `<html data-title-bar>`: `hidden` on macOS, `overlay` on Windows.
- `src/ui.css:24-30`: the header is the window drag region (`-webkit-app-region: drag`); every button/tab/link/field in it, and every layer, is `no-drag`. macOS: 88 px left padding for the traffic lights. Windows: padding from the Window Controls Overlay env vars (`titlebar-area-x/width`) + 16 px. Physical, not logical: an RTL app language does not move the window controls (#657). Full-height side panels start below the header.
- The desktop shell is **Electronite** (Graphite-enabled Electron fork) — required for Awami Nastaliq (see §11).
- `beforeunload` in a browser warns while any scheduler holds unsaved work; under the desktop shell the main process owns the "Unsaved changes" close dialog (`src/state.jsx:3579-3604`). `pagehide` drains every scheduler.

---

## 2. Navigation model

### 2.1 Levels

```
Home ──(card tile / resume / conflicts chip)──▶ Project
Project: mode switch  Understand ◀─▶ Translate ◀─▶ Check
                                                   ├─ tool picker
                                                   │   ├─ tW session  (rail + detail)
                                                   │   ├─ tN session  (rail + detail)
                                                   │   ├─ Align workspace (Bible only)
                                                   │   └─ Community Checking (view 'publish')
Bible:  book rail (books → chapter grid) on Understand and Translate
OBS:    story rail (stories → frame grid) on Understand and Translate
```

- There is no in-app back stack. "Back" controls are explicit: "← All checking tools", "← Back to translating", "Back to {Book} {chapter}" (conflict review), brand/project chip → Home.
- Every navigation is **flush-and-go**: `actions.go` first drains all five save schedulers; a failed write blocks the navigation (FR-32) (`src/state.jsx:3915-3935`).
- A mode switch is a **checkpoint**: after the switch, a git commit "Checkpoint, leaving {Understand|Translate|Check|Community Checking}: …" is started and not awaited; a failure lands in the save indicator as "Checkpoint not saved" (`src/state.jsx:3929-3933`). Leaving the project (Home) is also a checkpoint (`backToProjects`, `src/state.jsx:7028`). Typing never commits.
- Switching to Check from another mode re-reads the draft into an open check session (#532) (`src/state.jsx:3924-3925`). The open check tool stays open across a Translate ↔ Check switch; opening another book closes it (`src/state.jsx:6698-6703`).

### 2.2 Opening a project

- `openProject(repoPath, book, view = 'read')` (`src/state.jsx:2027`): a project with no remembered place opens in **Understand** (D87, #507).
- Home book tile → `openProjectAt(repoPath, code)` (`src/state.jsx:6586-6604`): if this client has a remembered place for that book (per client, `placeByProject`), opens in Translate, then restores chapter, then mode (`read` → Understand; `check` → Check and reopens the remembered tool via `resumeCheckTool`). Never-opened book → Understand, chapter 1.
- Story tile: same, restoring the frame (`setStoryFrame`).
- Resume card (Home banner) → opens the last edit's project/book in Translate, chapter, then restores mode and Check tool (`src/views/Home.jsx:279-290`).
- Slow open: after 300 ms a modal progress overlay shows (§3.12).

### 2.3 Book / chapter / verse navigation (Bible)

- **Book rail** (`src/views/BookRail.jsx`, `railChrome.jsx`): 230 px (`--rail-width`), white panel, end border. Header overline "Books · {n}". One row per project book: a `BookTile` row (name, percent, 5 px progress bar). The active book row and its chapter grid share one Inspire-50 tint group. Under the active book: a 5-column grid of chapter number buttons (32 px high, radius 8). Selected chapter = Inspire fill, white text; others white with tertiary text.
  - Book row click → `openBook(code)`: drains saves, closes an open check session if the book changes, loads the book, chapter 1 (`src/state.jsx:6685-6720`).
  - Chapter click → `setChapter(c)`: drains the note scheduler; clears editing and the helps focus (`src/state.jsx:6884-6893`).
  - Percent in rail: the active book live (`book.draftPct`), others from the Home progress cache; tooltip "{n}% of this book's verses are drafted".
- **Rail toggle**: an icon button (sidebar glyph, divider on the left) at the start of each view toolbar, `title="Toggle book list"`; toggles `s.rail` (shared by Understand and Translate; default shown) (`src/views/PanelIcons.jsx`; `src/state.jsx:6901`).
- Verse navigation is scrolling within the chapter; there is no verse picker. Check and Align have previous/next arrows and a rail.

### 2.4 Story / frame navigation (OBS)

- **Story rail** (`src/views/StoryRail.jsx`): same chrome. Header "Stories · {n}". One row per story: "N · Title" (the open story's live title, else the cached drafted/gateway title, else "Story N") with percent. The active story shows a 5-column grid of frame buttons: selected = Inspire fill; drafted = Inspire-50 tint with accent text; empty = white. Tooltips "Frame {n}, drafted" / "Frame {n}, not drafted" (`StoryRail.jsx:18-59`).
- Story click → `openStory(n)`; frame click → focus that frame and smooth-scroll its unit into view.
- Frame 0 is the story title; the reference line is not a frame.

### 2.5 Per-client remembered UI state (never in the project)

Stored in the platform's per-client settings document (`updateClientSettings`, `src/state.jsx:3642`):

- `lastUsed[repoPath]` — Home card order (newest of lastUsed / repo timestamp first) (`src/state.jsx:3822-3826`).
- `lastEdit` — the Resume banner: `{repoPath, book, chapter, verse, snippet (≤90 chars), at, mode, tool}`.
- `placeByProject[repoPath][book|story]` — mode, chapter/story, verse/frame, Check tool (#329).
- `obsStoryByProject`, `obsRecentByProject` — last story, recent stories (OBS card tiles).
- `draftUnits[repoPath]` — Section/Verse unit choice (shared by Understand's "Comment by" and Translate's unit switch) (`src/state.jsx:4065-4071`).
- `alignSuggestions[repoPath]` — the Suggest-links switch.
- `appLocale` — the app language.

In memory only (lost on restart): rail/helps visibility, helps width and tab, the Check tool's position per (tool, book) (`toolPos`), filter/sort in the check rail, the Community Checking page setup, the internet state (always off at launch).

---

## 3. Screens

### 3.1 S1 Home (`src/views/Home.jsx`)

Layout: `main` scrolls; content max width 980 px (`--measure-page`), padding 44/40/64.

1. `h1` "Your projects" (38 px `--fs-display`).
2. **Resume card** (only when `lastEdit` names an existing project) (`Home.jsx:272-311`): Ocean banner, radius 14, hero shadow, `role="button"`, Enter/Space activate. Title "{project} · {Book chapter}" or "{project} · Story N"; sub-line "Last edited verse {v} — “{snippet}”" (or "frame"), or "Last edited verse {v}" when emptied; a pill "Resume →" (Inspire fill).
3. Section header: overline "PROJECTS" (`h2`) + primary button "+ Add a project" (`data-testid="add-project"`).
4. Banners (warn Callouts, role alert):
   - Checkpoint failure on the project just left: "{error} Open the project again to retry." + "Ask for help" (`Home.jsx:348-354`).
   - Open failure: "**The project could not be opened.** {error}" + "Retry" when the listing itself failed (projects unknown) + "Ask for help" (`Home.jsx:355-369`). The diagnosis text comes from the refusal catalog (`refusal.*` keys).
5. "Loading projects…" while unknown; dashed empty box "No projects yet. Select + Add a project to start." when the list is empty.
6. Project cards (gap 18):

**Bible project card** (`ProjectCard`, `Home.jsx:192-268`), padding 22/24, radius 16:
- 44 px Ocean initials square; name (20 px black, project `dir`); direction badge "LTR" (neutral) / "RTL" (warn); "Imported" badge (accent-soft) for the project an import just made; "On Door43" badge (valid) when the repo has an `origin`; **conflicts chip** "N conflicts · Review" (warn brown `#7A4A06` on `#FDF1DE`, Kindle border, fork glyph) → opens the conflict review.
- Meta line: "{languageTag} · {n} book(s) · {n} in progress · Only on this computer | Shared at {owner/repo}".
- Right side: Share button (outline pill) "Share on Door43" or "Upload changes"; "Settings" (secondary).
- Book tile grid (`repeat(auto-fill,minmax(148px,1fr))`, gap 8): `BookTile` per book — name, percent (or "—" when unknown, never a false 0%), 4 px progress bar, and "N conflict(s)" with a Kindle dot under the bar when the book has verse conflicts. Tile click → `openProjectAt`.
- A dashed "+ Add a book" tile at the end → Add-a-book dialog.
- More than 12 books: once every percent is known, the card shows only in-progress books, with a ghost button "Show all {n} books →" / "Show in-progress books only →" (`Home.jsx:16, 260-265`).

**OBS project card** (`ObsProjectCard`, `Home.jsx:83-146`): same header plus an "OBS" accent badge; meta "{tag} · 50 stories · {n} in progress · {pct}% drafted". Story tiles (min 236 px): "N · Title" (or "Story N"), percent, meta "{pct}% · {date}" for recently edited stories. Collapsed it shows the three most recently edited stories (newest first; else stories 1–3); "Show all {n} stories →" / "Show recent stories only →". No Add-a-book tile.

7. **Import toast** (fixed, bottom-center): "{name} was imported · {books}" with Dismiss (`Home.jsx:385-389`).

Lazy loads per card: progress (`loadProgress`), shared state from the git remote (`loadShared`), conflict counts (`loadConflicts`).

### 3.2 S2 Understand — Bible (`src/views/Understand.jsx`)

Purpose: read the source passage and its helps before translating. The **only** write is the user-comment box (a grow-only `note.add` journal event).

Layout: [Book rail] | main | [Helps panel].

- Toolbar (white, hairline bottom, padding 10/26): rail toggle; `h2` "{Book} {chapter}" (17 px); subtitle "Read the passage and its helps before you translate." (ellipsis); helps toggle (`Understand.jsx:411-416`).
- Body: centered column max 720 px (`--measure-read`), padding 22/26/60.
  - Source chips row: one `FilterChip` per pinned `extraScripture` pane (ids from the project's pins; usually `ULT`, `UST`). Selected chip = Inspire fill, white text; unselected white with heading text. Label is `source.{id}` (e.g. "ULT"), else the id uppercased (`Understand.jsx:420-432`). Click → `setSourceTab` (drains notes first).
  - At the end of the row: overline "COMMENT BY" + a small Ocean pill switcher **Section | Verse** (persisted per project as `draftUnits`). In a cross-frame project (project versification differs from the source) the switcher is replaced by "Verse view. This project's verse numbering differs from the source, so sections are not available."
  - Under the chips: the full source name, e.g. "unfoldingWord Literal Text".
  - Status callouts (`PassageStatus`, `Understand.jsx:322-365`): pins read error + Retry; comment save error; "This project pins no source texts…"; source absent ("This source is not available for this book." / "This source text is not on this computer."); source read error + Retry; "Loading the passage and its helps…".
  - **Units** (`understandUnits`, `Understand.jsx:212-224`): Section mode = one unit per `\ts\*` section of the active source (or the whole chapter if the source has none); Verse mode = one unit per verse key; cross-frame = one unit per mapped project verse, with info callouts for unmapped verses ("Project verse {ref} has no place in the source's verse numbering…") and cross-book mappings.
  - Each unit (`UnderstandUnit`, `Understand.jsx:262-302`): radius 14, padding 12/16/14. Focused unit = white card with a 1 px Inspire-35% ring and soft shadow; others flat. Header overline "Titus 2:1–4" + a green status dot when the unit has a saved comment. Source paragraphs in Charis SIL 22/36 px, verse numbers as 11 px bold superscripts, poetry indents 1.5em / 3em, paragraph breaks from USFM `\p \q…` markers. Words quoted by the hovered/active help card are highlighted (`<mark>` on `--tc-highlight-soft` #FFF1B8).
  - Unit click, or Enter/Space on the focused unit, focuses it; focusing also selects the first help card (notes first, then words) in that unit's verse range, or clears the help selection (`Understand.jsx:400-405`).
  - **Comment box** (`ComprehensionBox`, `Understand.jsx:77-182`): a 3-row textarea in the project's script face at verse size, project `dir`, paper background. Disabled until the stored comments have been read. Placeholders: Section "What is this section saying? Who is speaking, to whom, and what will be hard to render?"; Verse "What does this verse mean, and what will be hard to render?"; Frame (OBS) "What does this frame say, and what will be hard to render?". Every keystroke stages into the note scheduler; blur flushes.
    - Comments cannot be emptied once saved: emptying and leaving restores the saved text and shows "A saved user comment cannot be emptied — user comments are permanent in this version. The saved text was restored; edit it instead." (`Understand.jsx:138-145, 168-172`).
    - When several saved comments fall inside one section and none matches the section head, the box shows none and says "{n} more saved user comment(s) target other verses inside this section — switch to Verse view to see and edit each one." (`Understand.jsx:173-179`).
- Helps panel without the Comments tab (§4).

### 3.3 S3 Translate — Bible (`src/views/Draft.jsx`)

Purpose: draft the translation beside the source. Internal view id `draft`.

Layout: [Book rail] | main | [Helps panel with Comments tab].

- Toolbar: rail toggle; `h2` "{Book} {chapter}"; spacer; **conflicts button** "N conflicts · Review" (when the project has verse conflicts; disabled while an editor is open); helps toggle (`Draft.jsx:328-334`).
- Body: a centered two-column grid, max 1100 px, `1fr 1fr` (`Draft.jsx:337`).
  - Sticky header row:
    - Left (source): chips for each `extraScripture` pane plus an **original-language chip** ("Greek" or "Hebrew") when the original is loaded (`SourceTabs`, `Draft.jsx:224-254`). Caption: "{source full name} · {version} · pinned".
    - Right (target): accent overline "{project name} · {language tag}"; a Section/Verse pill switcher (disabled while an editor is open; pointerdown is swallowed so the textarea keeps focus) or the cross-frame "Verse view…" note (`DraftUnitSwitch`, `Draft.jsx:262-287`).
  - One row per **section** (the active source's `\ts\*` starts, else the book's own `\ts\*`, else the whole chapter; cross-frame: one per verse) (`chapterSections`, `Draft.jsx:92-101`):
    - Source cell: overline "{Book} {c}:{span}", paragraphs of the source verses (Charis 22/36; Hebrew RTL `lang=hbo` in Noto Serif Hebrew; Greek `lang=el` in Noto Serif) with helps-quote highlighting; an absent source shows its reason in italics; a missing verse shows "source text loads with resources".
    - Target cell: in Section mode a ghost button "Draft section {span}" above the text; drafted words, each wrapped in a quiet hover span; verse superscripts; paragraph/poetry indents.
      - Verse mode: a drafted verse is clickable (cursor text, title "Edit this verse") → verse editor; an undrafted verse shows a dashed pill "Draft verse N" (accessible name "start this verse").
      - A verse with an open editor shows an inline accent pill "Editing below".
      - A verse with a conflict is tinted warn and carries a marker button "{n} versions · Review" (fork glyph), aria-label "{ref} has {n} versions. Review them." (`src/views/conflictMarkers.jsx:47-67`).
- **Verse editor** (`VerseEditor`, `Draft.jsx:33-77`; shell `EditingCard`, `src/views/draftChrome.jsx:38-46`): a card with a 1.5 px Inspire border, radius 10, soft blue shadow. Header "DRAFTING {n}". Textarea (3 rows) autofocused, `aria-label="Verse {n}"`, the project `dir` and script face at the same size as the reading text, placeholder "Type your translation of this verse…". Every keystroke splices into the in-memory USFM and marks the scheduler dirty (`editVerse`, `src/state.jsx:6921-6935`); emptying a verse stores the stub `___`. Blur saves and closes. Footer: "Save verse" and ghost "Cancel" (restores the pre-edit body). Both buttons `preventDefault` on mousedown so the textarea's blur does not swallow the click.
- **Section editor** (`SectionEditor`, `src/views/SectionEditor.jsx`), opened by "Draft section {span}":
  - Header "DRAFTING {span}" + small pill switcher **Type | Place verse numbers**; in Type mode the hint "Enter for paragraph, Tab for poetry. Place the verse numbers next."
  - **Type mode**: a 6-row textarea in one step smaller type (`--fs-verse-md` 19.5 px), `aria-label="Section {span}"`, placeholder "Type your translation of this section…". The text holds one "N body" line per verse. A blank line before a verse = new paragraph (`\p`); one leading Tab = `\q1`, two = `\q2`. Tab / Shift+Tab add or remove one leading tab on the caret's line (max 2) (`SectionEditor.jsx:172-183`; `sectionDraft.js:70-94`). A body line that starts with a verse number is escaped with one leading space.
  - **Place mode** (`PlaceView`, `SectionEditor.jsx:100-152`): a warm "TO PLACE" bank (cream, warn border) holding the unplaced verse pins (`VerseMarker`: a Kindle map-pin with the number, 24×32 px). Instructions: "Click a verse number, then click the word that begins the verse." / "Click the word where verse {n} begins" / "Every verse is placed. Click a verse number to move where its verse begins." The words of the section are spans. The first verse is fixed at word 0 (plain superscript, title "Verse {n} begins the section and is fixed").
    - Pick a pin (pointerdown or click, Enter/Space) → it shows "dragging" (stronger shadow); hovering a legal word shows a ghost pin before it and highlights the span the verse would cover; the word title reads "Begin verse {n} at {w}" or "Join verse {n} to verse {m} at {w}"; illegal targets show "Verse {n} already begins here" / "Verse {n} cannot pass another verse number" and a not-allowed cursor.
    - Drop: click or pointer-up on a legal word, or Enter/Space on a focused legal word (legal words become `role="button"` and tabbable while a pin is held); Escape puts the pin back. Clicking the held pin again puts it back.
    - Two pins on one word form a **verse span** (`\v 9-10`); moving a pin off breaks the span. Pins never pass each other.
  - Footer: "Save section" (disabled when nothing would change) and "Cancel". Save writes each changed verse through `editVerse`, or one structural action when the verse keys changed (span created/broken), then re-flows alignments (`saveSection`, `src/state.jsx:6961-6999`).
  - Saving a section with no words asks first: modal "Remove all the text of section {span}?" / "The section has no text now. If you save it, each verse of the section goes back to not drafted." Buttons "Keep editing" / "Remove the text" (`SectionEditor.jsx:244-255`).
- Only one editor (verse or section) is open at a time (`s.editing`). While one is open, the unit switch and conflict entries are disabled.
- After a verse edit, alignments whose target words remain are kept, the rest go back to the word bank (`reflowAlignedVerses`, #213).

### 3.4 S4 Conflict review (`src/views/ConflictReview.jsx`)

Shown inside Translate in place of the two-column grid while `s.conflictReview` is set (`Draft.jsx:300`). Reached from the Home conflicts chip, the Translate toolbar button or a verse marker.

- Header bar (53 px, white): ghost "← Back to {Book} {chapter}" (or "Back to Translate"); divider; `h1` "Review conflicts"; right side "Conflict {n} of {m}" (`aria-live="polite"`) + "Previous" / "Next" (disabled at the ends and while a keep runs).
- Body (max 860 px): `h2` "{Book} {c}:{v}" + "{n} versions of this verse" + legend swatches "Mine" (Inspire-200 `#a8def4`) and "Teammates" (cream `#F6EEDC`).
  - The ULT verse for reference, when the conflict is in the open book and the pane is same-frame (`UltReference`).
  - One **version card** per version, newest first (`VersionCard`, `ConflictReview.jsx:57-84`): a button with `aria-pressed`, background by owner (Mine light blue, any teammate cream); a 22 px radio-like circle (Ocean filled check when selected); owner pill ("Mine" on Ocean, or the teammate's Door43 name, or "Teammate n"); date "14 Oct 2026 · 09:12"; the text in the project face. Selecting a card fades the others (darker text color, not opacity) and gives the selected one a 2 px Ocean border and lift.
- Footer bar (68 px): hint "Click the version to keep." or the consequence "**Your version** replaces the other version. It stays in the project history." / "…replaces the other {n} versions. They stay in the project history."; "Cancel" (clears the selection); primary "Keep this version" (disabled until a selection) → `keepVersion` writes one verse edit replacing every version.
- Errors: "**The version was not kept.** {error}"; after a keep that worked but the book did not reload: "The book did not open again. Go back to Home and open the project again."
- "No conflicts are open." when the list is empty.

### 3.5 S5 Check — tool picker (`src/views/Check.jsx:1343-1448`)

- Page max 980 px, padding 44/40/64. `h1` "Choose a checking tool" (32 px); subtitle "One focused pass through the whole book. Your place is saved in each tool."
- Preflight runs on entry (and on book/pins change) and resolves each derived tool to a state (`runPreflight`): `ready`, `fetch`, `unavailable`, `unpinned`, `not-covered`, `old-twl`.
- Banners: preflight read error + Retry; "A pre-release version of translationCore pinned the Translation Words links of this project…" (old twl pin); "Checking what is available…".
- Card grid `repeat(auto-fit,minmax(270px,1fr))`, gap 16. Card style: radius 16, padding 22, card shadow, 19 px black title, description, progress block, CTA.
  1. **Translation Words** and 2. **Translation Notes** (`ToolCard`, `Check.jsx:195-246`):
     - Ready: plain white card (`data-i="card"` lifts on hover); description "Check the key terms in your translation against their definitions." / "Work through the notes that link to a Translation Academy article."; progress bar (valid green, 6 px), "{decided} of {total} resolved", "Next: “{quote}” · {c}:{v}" or "All resolved — review any item" or "No items in this book yet"; CTA button "Start checking →" / "Continue →" / "Review →" (or "Open this tool →" when progress is unknown). The whole card is clickable too; the CTA is the real button and first Tab stop.
     - Ready with the primary missing (English fallback in use): the CTA first, then a warn callout "This project pins {repo} · {version} for the primary language, but it is not on this computer. Checking uses the English fallback. Download the pinned resource to check against it." + "⇣ Download it" (opens Source texts), then progress.
     - Not ready: tinted card with an uppercase state label ("Needs downloading", "Unavailable in Local", "No resources pinned", "Not available for this book", "Create the project again"), an explanation, the pin `repo · version · primary gateway language|English fallback` in mono, and one action: "Fix this…" (fetch/unavailable → Guided fix; Kindle fill when unavailable) or "Get source texts" (unpinned/not-covered → Source texts).
  3. **Align** (Bible only; absent for OBS): "Link your words to the original language. Alignment powers the highlights that show which words a check is about."; progress "{done} of {total} verses aligned", "Next: verse {ref}"; whole-card button (role button, Enter/Space).
  4. **Community Checking**: "Read the book as a whole and share it with the community for feedback. Export a copy for reviewers."; an empty progress bar; "Whole chapter" / "Next · read {Book chapter} aloud" (OBS: "All drafted stories" / "Next · read the drafted stories aloud"); CTA "Open →" → view `publish`.
- Footer note when any tool is not ready: "A tool that is not ready never blocks the rest of your work. Drafting, other books, and the other tool carry on."
- Focus: after "Back" from a session, focus returns to the first enabled button of the card that opened it, if focus is still on the body (`Check.jsx:1368-1377`).

### 3.6 S6 Check — tW / tN session (`src/views/Check.jsx:1196-1294`)

Opening a tool derives its item list from the pinned TSV (never stored) and opens at the remembered item (same chapter:verse + groupId + occurrence), else the first undecided, else item 1 (`checkStartIndex`, `src/state.jsx:623-628`).
States: "Deriving the check list…"; error text; designed empty states (§3.6.4).

Layout: [Check rail 300 px] | [Detail column, max 760 px] (+ Academy drawer).

#### 3.6.1 Check rail (`CheckRail`, `Check.jsx:890-1012`)

- Header: ghost "← All checking tools" (`check-back`); `h2` tool name; book label (or "Story N"); 7 px progress bar; "{decided} of {total} resolved".
- **Filter chips** (pills, Ocean fill when on): "All · n", "To do · n", "Invalid · n", "Bookmarked · n", "Commented · n". Counts and filtering share one predicate table (`src/views/checkFilters.js`).
- **Sort** (overline "SORT"): tW "By word" (default; grouped by term, alphabetical) | "By verse"; tN "By verse" (default) | "By category" (grouped by tA category). For a story "By verse" reads "By frame". Filter and sort reset when the tool or book changes.
- Versification-dropped note: "{count} check(s) from this resource have no verse in this project's {scheme} numbering, so they are not shown and not counted. Nothing was lost…".
- Resource citation in 9 px mono: "{repo} · {version} · primary gateway language|English fallback".
- Item list (one Tab stop, roving tabindex): group overline (book name, term, or category), then one button per item: a 9 px status dot (valid green #58C17A, invalid red #A21309, to-do steel; **invalidated items show the red dot**), the gateway title of the quote in quotes (ellipsis), ⚑ bookmark glyph (Kindle), ✎ comment glyph, and the ref "c:v". The active row has an Inspire border and Inspire-50 fill, `aria-current`. Title tooltip "{locator} · {groupId}".
  - Keys: ArrowUp/ArrowDown move the selection (wrap), Home/End jump; focus follows (`railKeyDown`, `Check.jsx:875-888`).
  - When a session opens, focus moves to the current rail item once, only if nothing else holds focus (`useFocusOnOpen`, `Check.jsx:1182-1194`).
- "Nothing matches this filter." when a filter empties the list.
- Footer: ghost "← Back to translating" — closes the tool and goes to Translate. Deliberately out of the Tab order (`tabIndex=-1`) (`Check.jsx:1005-1008`).

#### 3.6.2 Detail column (`CheckDetail`, `Check.jsx:624-714`)

1. Header row: "{Book} {c}:{v}" (or "Story N c:v"); "Item {n} of {total}"; ← / → 30 px square buttons ("Previous item" / "Next item"; disabled at the ends).
2. `h1` the quote in quotes, Charis 32 px bold. The title is the **gateway rendering** of the original quote, derived through the ULT pane's alignment; it falls back to the original-language quote, then the groupId (`gatewayTitleFor`, `Check.jsx:1159-1172`).
3. "WHAT TO CHECK" box (cream, warn border, radius 12, max 62ch): the note body (tN occurrence note, tW…) as rendered markdown at 16 px; long notes (>400 chars) collapse with "Show more" / "Show less" (`ExpandableNote`). Direction = the helps' gateway package direction.
4. Ghost button "Learn more: {article title} →" → **Translation Academy drawer** (tN → tA article; tW → tW article) (§3.6.3).
5. Notices (`CheckSessionNotices`, `Check.jsx:716-748`):
   - Resolution warning (warn callout): "These checks were made against a different version of this resource." or "…a different resource." + mono "checked against {stored} · now pinned to {now}".
   - Save refused (red box): "This check was not saved. The project's resource changed since this book was checked — update the resource first, then check again. Nothing was lost."
   - **Invalidation notice** (red box): "{n} decision(s) no longer match the draft text and need re-review. Nothing was discarded."
6. **Compare card** (white, radius 14, card shadow):
   - Original-language row ("ORIGINAL LANGUAGE · GREEK|HEBREW"): the full source verse, Noto Serif / Noto Serif Hebrew, `dir` RTL for Hebrew; or "— not in this verse —", the cross-frame note, a read error, or "The original-language text is not on this computer." (`OrigPane`, `Check.jsx:404-430`).
   - Gateway row on paper tint ("GATEWAY · UNFOLDINGWORD LITERAL TEXT" or "GATEWAY · {PANE}"): the verse with the quote's words highlighted (`<mark>` #FFF1B8) (`UltPane`, `Check.jsx:461-501`).
   - OBS: instead of both rows, "GATEWAY · STORY TEXT" with the frame text and the help's source phrase highlighted by plain word match (`StoryPane`, `Check.jsx:507-530`).
   - "YOUR TRANSLATION" (accent overline): the target verse as tappable word buttons at 19 px Charis, line-height 1.9 (`TargetWords`, `Check.jsx:558-619`). Selected words get the highlight-yellow fill (`aria-pressed`). Hint "Tap the word(s) in your translation that render the highlighted quote." Undrafted: "This verse is not drafted yet. Draft it first, then return to check." (OBS: "This frame is not drafted yet…"). The selection restores from the stored record when the item or its text changes.
7. **Triage buttons** (`Check.jsx:696-709`), large blocks (radius 10, 14 px heavy, padding 11/20), each filled when it is the current status:
   - "✓ Mark valid" (valid green #327547) → status valid with the selected words; with no words selected → `nothingToSelect: true`.
   - "Invalid" (red #A21309) → status invalid, selections false.
   - "To do" (black) → status todo.
   - There is **no auto-advance** after a decision; the user moves with the rail, the arrows or the keys (`recordDecision`, `src/state.jsx:5541-5580`).
   - A decision on an item that was invalidated by a draft edit or language change and is marked Invalid by the user keeps a `userInvalid` mark (#580).
8. **Marks** (`CheckMarks`, `Check.jsx:768-827`): a chip "⚑ Bookmark" / "⚑ Bookmarked" (toggle; tooltip "Bookmark this check to come back to it" / "Remove this bookmark"), and right-aligned "✎ Add a comment" / "✎ Comment · written" / "✎ Hide comment". The comment editor: overline "MY COMMENT ON THIS CHECK", a 3-row textarea (`dir="auto"`), placeholder "Say what you decided, or what to ask your team about this check.", buttons "Done" and ghost "Delete comment". Done/Delete return focus to the comment toggle. Bookmarks and comments never move progress.

Every decision, bookmark and comment is written as a §5.2 record through the check scheduler, flushed at once.

#### 3.6.3 Translation Academy drawer (`AcademyDrawer`, `Check.jsx:339-349`; DS `Drawer`)

- End-edge slide-over, 440 px (`--drawer-width`), drawer scrim `rgba(1,66,99,.32)`, slides over 240 ms, traps focus, Escape and scrim close it, focus returns to the "Learn more" button.
- Eyebrow "TRANSLATION ACADEMY · {category}", title = article title (direction of its gateway package), close ✕ ("Close").
- Body: rendered article blocks (headings Ocean 12.5 px heavy; paragraphs 15 px, line-height 1.7; list items indented 14 px; `>` quotes as nested 3 px left bars; bold/italic spans), then the article path in 9 px mono. "Loading the article…"; read error; "The pinned resources do not carry the article for this check. The check still works; the explanation is missing." The article is never truncated.

#### 3.6.4 Designed empty states (`CheckEmpty`, `Check.jsx:353-376`)

A dashed box with title + body for `none` ("No checks for this book"), `all-dropped` ("No checks line up with this project's numbering"), `missing` ("This book is not in the pinned resource"), `versification-unavailable`, `versification-unknown`; the dropped note; the resource citation; a "← All checking tools" button.

#### 3.6.5 Editing a verse from Check

There is no inline verse editor in Check. The user goes to Translate (the mode switch or "← Back to translating"), edits, and returns. The open tool stays open across the mode switch and re-reads the draft on return (#532; `e2e/check-returns-draft.spec.ts`). A decision whose selected words left the draft is flagged invalidated (red dot, red notice, "Invalid · n" filter) and kept for re-review, never deleted (J6).

### 3.7 S7 Check — Align workspace (`Check.jsx:1014-1130`, `src/views/Align.jsx`)

Opened from the Align card (`startAligning`, `src/state.jsx:5042-5058`): opens at the remembered verse, else the picker's next unaligned verse, else the first drafted verse; the suggestion engine retrains.

**Align rail** (300 px): "← All checking tools"; `h2` "Align"; book name; progress "{done} of {total} verses aligned" over drafted verses; filter chips "All · n", "To do · n" (todo + undrafted), "Invalid · n"; index error line; one row per verse of the book: status dot (valid, invalid, todo, undrafted grey), the ref, the verse text in the project `dir` (ellipsis) or italic "Not drafted yet" (undrafted rows disabled at 55% opacity); footer "← Back to translating" (`AlignRail`, `Check.jsx:1027-1107`).

**Editor** (max 1280 px) (`Align`, `Align.jsx:318-449`):
1. Header: "{Book} {c:v}", "Item {n} of {total}" over drafted verses, ← / → buttons; `h1` "“{Book} {c:v}”".
2. "ⓘ How alignment works" disclosure (`aria-expanded`): "Connect each word of your translation to the word it renders. Click a word in the bank, then click its card — or drag it. Drag one card onto another to align a phrase; SPLIT undoes it. Every other check uses these links to find your words."
3. Stale warning (cream): "The draft changed since this alignment was made. Check that the words still line up."
4. The editor card (white, radius 14):
   - **Mode bar**: pill toggle "Gateway · ULT" | "Original · Greek|Hebrew" + hint. Gateway mode is a lens: cards are labelled by the ULT words aligned to each original word, with the original as a sub-label. It is offered only for a same-frame (eng) project whose ULT verse carries alignment data; otherwise disabled with "The gateway lens needs the gateway source text with alignment data for this verse." (`ModeBar`, `Align.jsx:223-243`).
   - **Reference band**: the anchor verse text (ULT or original).
   - **Suggestions row** (`SuggestionsRow`, `Align.jsx:475-509`): overline "SUGGESTIONS"; checkbox "Suggest links" (per client per project); status text — "Off. Turn on to get links proposed from the verses you have already aligned." / "Reading your aligned verses…" / "No aligned verses yet in this testament — align a verse by hand first." / "Drawn from {n} verses you have aligned — suggestions improve as you align more." / error; when on, buttons "Suggest", "Accept all", "Reject all", "Reset". "Nothing to suggest for the words still in the bank." A suggestion is never saved until confirmed.
   - **Word bank** ("WORD BANK · YOUR TRANSLATION", paper tint, `dir` = project): one button per unplaced target word (Charis 15 px, Inspire 1 px border, radius 8). Click arms the word (Inspire fill, focus shadow; `data-armed`); click again disarms. Draggable. "{placed} of {total} words placed". Empty: "Every word is placed. Click a placed word to send it back." The bank is a drop target for placed words (un-align).
   - **Word cards** (one per alignment group; `dir` = original): min width 78 px, 1.5 px border, radius 10. Header (draggable onto another card to **merge** into a phrase; tooltip "Drag onto another word card to merge them into a phrase") shows the original word(s) (Noto Serif 17 px bold) or the gateway lens label; a merged card shows a "SPLIT" button ("Split back into single words"). Body: placed target words as chips (click = un-align, "Click to un-align"; draggable to another card or back to the bank); suggested words as dashed `WordChip`s with "✓" (click = confirm); an empty dashed slot when nothing is placed. While a word is armed every card highlights as a target (Inspire-55% border) and a click places the word; a drag-over highlights the card (Inspire border + Inspire-50 fill).
   - Drag payloads are JSON in `text/plain` (`bank`, `placed`, `card`); malformed drops are ignored (`Align.jsx:26-34, 350-365`).
5. **Mark valid** (`MarkValidBar`, `Align.jsx:515-537`): "✓ Mark valid" — the translator declares the verse done even with words left in the bank (tooltip "This verse's alignment is done, even if some words stay unplaced. Any later change takes it back."). Filled green while done, not stale and not flagged invalid. Refused while suggestions stand: "Confirm or reject the suggested links first."
- Unavailable states (dashed box, title + body): nothing drafted, no original pinned, original not on this computer, versification data not loaded, no single source verse (cross-frame), versification unknown, unreadable source; read error + Retry; "Opening the alignment…" (`AlignBlocked`, `Align.jsx:162-198`).
- There is no Align tool for OBS projects (D74 §8).

### 3.8 S8 Community Checking — Bible (`src/views/CommunityChecking.jsx:159-217`)

Typeset preview of the whole open book as paged sheets, plus export.

- Main (Frost `#ECF2F5` background, padding 34/24/60): `PrintPages` (§12) — paper sheets (A4 210×297 mm or Letter 215.9×279.4 mm at 96 dpi; margins top 18, side 16, bottom 20 mm), white, page shadow, radius 4, page number centered in the bottom margin (Charis 11 px, `#626F78`). The first sheet carries the book title. Chapters with a drafted verse print; runs of undrafted chapters between them are one line "[ chapter {n} not yet drafted ]" / "[ chapters {from}–{to} not yet drafted ]"; undrafted verses print "[ verse not yet drafted ]" in grey; undrafted chapters before the first / after the last drafted chapter are omitted. "Nothing is drafted in this book yet." when empty.
- Side panel (300 px, white, start border, padding 22):
  - ghost "← All checking tools"; `h2` "Community Checking".
  - **Export menu** (`src/views/ExportMenu.jsx`): a large primary button "Export" opening a DS `Menu` with the producers that apply: "Export PDF" (desktop only — needs the PDF bridge), "USFM, aligned", "USFM, plain", "Scripture Burrito (.zip)" (Bible); OBS: "Export PDF", "Story Markdown (.zip)", "Scripture Burrito (.zip)". File name `{subject}-YYYY-MM-DD.{ext}` (`src/data/export/kernel.ts:40-43`). Results: desktop toast "Saved {filename}" (only after the download completed), "{filename} did not finish downloading. Export it again." (interrupted), browser "Export ready: {filename}"; failure callout = diagnosis + the refusal's recovery sentence (e.g. "Draft at least one verse of this book, then export the PDF again."). No producer applies → caption "Exports arrive later in this increment (J7)."
  - **Page setup** box (paper tint, radius 12): rows of choice chips — Columns "1 | 2", Spacing "Single | Double", Paper size "A4 | Letter"; toggles "Drop-cap chapters" (default on), "Verse numbers" (default on), "Footnotes" (disabled, not built). Defaults `{columns 1, dropCap true, verseNumbers true, spacing single, paper a4, pictures true, obsLayout above}` (`src/data/export/pageSetup.ts:22-30`). The page setup lives in memory only and is passed to every export.
  - Kindle callout when anything is undrafted: "**Incomplete draft.** Undrafted verses and chapters show as not yet drafted. Undrafted chapters before the first drafted chapter or after the last are left out."
- Community Checking has no Share action (`e2e/j11-share.spec.ts` "Community Checking has no Share").

### 3.9 S9 OBS Understand (`src/views/StoryUnderstand.jsx`)

- Layout: [Story rail] | main (max 720 px) | [Helps panel, story mode, no Comments tab].
- Toolbar: rail toggle; `h2` "Story N"; "Read the story and its helps before you translate."; helps toggle.
- One unit per frame, frame 0 = title (`storyUnits`, `StoryUnderstand.jsx:24-32`): header overline "Story title" / "Frame N" + green dot when commented; overline "GATEWAY STORY"; the frame picture floated inline-start (30% width, min 160, max height 210, radius 10) with the gateway text beside and under it; the comment box (frame placeholder), keyed `{story, frame}`.
- Clicking a unit focuses it (and reports the frame to app state; the helps panel scrolls the frame's first card into view and marks the frame's cards active).

### 3.10 S10 OBS Translate (`src/views/StoryDraft.jsx`)

Mirrors Bible Translate (#307 parity):
- Toolbar: rail toggle (`title="Toggle story list"`), `h2` "Story N", helps toggle.
- Notices: gateway story absent — "No gateway story is pinned for this project." / "The pinned gateway story {pin} is not on this machine." + "Get source" / mismatch / read error; pictures missing — "No picture on this machine matched the {n} image lines of this story: {per-pack diagnosis}" (`StoryNotices`, `StoryDraft.jsx:171-191`).
- Two-column grid (max 1100): source header = an "OBS" badge where the Bible tabs sit + "Gateway story · {version} · pinned"; target header = "{project} · {tag}" (no unit switch).
- Units in file order: title (frame 0), each frame, the reference line (`storyUnits`, `StoryDraft.jsx:54-70`). Source cell: label, frame picture (full width, max 210 px high, cover), gateway text (or italic "Gateway text is unavailable."). Target cell: drafted text (`role="button"`, tabbable, Enter/Space or click to edit, title "Edit this text") or dashed pill "Draft frame N" / "Draft the title" / "Draft the reference".
- Unit editor (`UnitEditor`, `StoryDraft.jsx:77-107`): `EditingCard` "DRAFTING {label}"; textarea (frames, 4 rows) or input (title, reference); placeholder "Type your translation…"; each keystroke stages; blur saves and closes; "Save" / "Cancel" (stages the original text back so nothing is written).
- Helps panel with Comments tab, focus on the focused frame.

### 3.11 S11 OBS Check

The same picker and session as §3.5–3.6, scoped to the open story (book position `OBS`): only Translation Notes, Translation Words and Community Checking cards (no Align). Unit label "Story N"; sort "By frame"; compare card shows the gateway story frame only; target text is the drafted frame (`Check.jsx:38-45, 503-542`).

### 3.12 S12 OBS Community Checking (`CommunityChecking.jsx:96-157`)

- Main: one page per drafted story (max 680 px, padding 64/72): eyebrow "PREVIEW", title (Charis 38 px bold, centered), a 70 px rule, each drafted frame's picture and text (justified, 19.5/32), one line per run of undrafted frames ("[ frames {from}–{to} not yet drafted ]"), the reference line in italic. Runs of undrafted stories between drafted ones are one centered line. "Nothing is drafted in this project yet."; "Loading stories…"; "The stories could not be loaded. {error}".
- Side panel: Back, title, Export menu, Page setup with Layout "Pictures above | Pictures wrapped" and toggle "Pictures". Wrapped: the picture is 25% wide, floated to the start corner (left for LTR, right for RTL), text wraps. Kindle callout "**Incomplete draft.** The export shows each run of undrafted frames or stories as one line, without pictures…".

### 3.13 Open-progress overlay (`src/views/OpenProgress.jsx`)

- Nothing for the first **300 ms** of an open (`SHOW_THRESHOLD_MS`, line 19). Then a centered modal (modal scrim, no dismiss, focus trapped, Tab swallowed): `h3` "Opening the project…"; a live caption with the stage — "Reading the journal · {done} of {total} saved actions" (determinate bar, floor percent) / "Checking the project state" / "Preparing the project" (indeterminate) (`OpenProgress.jsx:21-73`). A failed or superseded open clears it.

### 3.14 Dev Inspector (`src/views/dev/Inspector.jsx`)

Dev builds or `VITE_TC4_INSPECTOR=1` only. **Alt+Shift+I** toggles a 520 px fixed panel at the bottom end: "Inspector (dev)", write errors, "Last Report" JSON, "Ops log" table (time, op, ok/code/open, repo · rolled back). Read-only. Not a parity requirement for end users.
