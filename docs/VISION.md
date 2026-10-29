# Vision

**Answers:** what translationCore 4 is, who it serves, and how to choose when two good things
conflict. **Wins over:** every other document on product direction. **Does not cover:** what a
person does step by step (`docs/JOURNEYS.md`), the promises every change keeps (`docs/PLAN.md`), or how it is
built (`docs/ARCHITECTURE.md`).

The product vision was confirmed by the owner on 2026-08-29 for tC4 (the JavaScript app, branch
`legacy-4.0`, `docs/VISION.md`). This app keeps it. The section "What the rebuild adds" is the
rebuild's own. Since the rebuild was finished, tC4 4.0 is history: this vision and tC4's design
system guide every new feature (ruling `docs/rulings/2026-10-29-tc4-is-history.md`).

Other documents cite a commitment by its id, for example "serves V1".

## What tC4 is

tC4 is an **offline-first desktop workspace** where mother-tongue translators understand,
translate and check Bible or Open Bible Stories (OBS) translations in one durable project.

It puts trusted resources beside the translator's text, keeps their work above all else, lets a
team exchange work through Door43 or a file, and produces readable deliverables: USFM and PDF for
a Bible, Markdown and PDF for OBS.

## Who it serves

Translators who draft or check in their own minority language from a major gateway language.
Some are experts; others need much help. The app shows the information for each passage without
forcing either group through gates. Facilitators and consultants support the work, but the
translator's needs decide every trade-off.

## Commitments

| Id | Commitment | What it means |
|---|---|---|
| V1 | **Keep the work.** | Translators may risk their lives for this work. Data loss is not an ordinary defect. The app never silently loses, rewrites, hides or strands a translator's text, alignment, check decision or comment. One project holds the text, pins, comments, decisions, alignments, history and team state. |
| V2 | **Work offline.** | Once a project and its resources are on the computer, everything except sending and receiving works with no internet. The internet is off until the person turns it on. |
| V3 | **Exchange work dependably.** | One person works on one computer while teammates work on their own. They exchange work through Door43 or a file on a USB drive, keep working offline, and never lose either side's work. Two versions of verse text are both kept and shown; for everything else the newest wins and the older stays in History (ruling H5). |
| V4 | **Keep resources explicit.** | The core helps are ULT, UST, tN, tW, tA and tQ, from Door43 or Aquifer. Resources stay pinned. The app announces newer versions and never installs them silently. An upgrade says what it will invalidate, keeps notes and decisions, and marks what needs another look. |
| V5 | **Show the writing system correctly.** | Right-to-left and complex scripts edit and print correctly, including SIL Graphite fonts. The interface itself runs right to left in Arabic. |
| V6 | **Keep projects portable.** | Every project on disk, and every project that leaves the app, is a valid Scripture Burrito. USFM, Markdown and PDF are deliverables. Projects open in later versions. |
| V7 | **Understand, Translate, Check.** | The three activities (UTC). Translators move among them freely. See below. |
| V8 | **Feel like tC4.** | Every screen, new or old, is built from tC4's design system: its look, components and the style of its words, so a translator who knows tC4 needs no new training. tC4 4.0's behavior does not bind: where it works against what a feature is for, follow what the translator needs, and list a change to anything that came from tC4 in `docs/DIFFERENCES.md`. |

### V7 in detail: Understand, Translate, Check

- **Understand** helps translators grasp the passage, find its translation issues and record
  shared notes. Preparation is encouraged, never enforced.
- **Translate** makes natural composition easy and hides file management. Bible translators work
  by verse or by section; OBS translators work frame by frame. Helps and notes stay close.
  Revisions never rewrite untouched text.
- **Check** pivots by concept: a term, a name, a metaphor or a translation issue across all its
  places. translationWords and translationNotes are the backbone. Translators record decisions,
  edit the text from the check, and align words to the original language. Community checking and
  preparing deliverables live inside Check.

## What the rebuild adds

The rebuild gave a translator the same outcome as tC4 4.0, plus three things:

1. **Team work goes further** (V3): exchange by USB drive or file; saves that continue while a
   sync runs; private repositories; any default branch; OBS projects sync; every kind of
   difference is either merged, shown as a conflict, or kept in History; carried files such as
   audio travel with the project.
2. **Helps from Door43 or Aquifer** (V4).
3. **Every project is a valid Scripture Burrito** (V6), on disk and when it leaves the app.

## How to choose

Four commitments never bend: **V1, V2, V5, V6.** A feature that weakens one of them is redesigned
or dropped.

Inside those, prefer in this order:

1. Understanding and translation quality (V7).
2. Low effort for the translator and a simple interface (V8).
3. Offline team work and exchange (V3).
4. Settings, more resources and secondary flows (V4).

## Not this

tC4 is not a general Bible-study app, a program-management suite, a live shared editor, an
anonymity guarantee, an automatic AI translator, a replacement for training or human review, or a
desktop-publishing system. PDF options are document-level only: columns, spacing, paper and text
size.

## Later, not promised

These fit UTC but are experiments: automated basic checks with live reports, audio drafting,
speech-to-text for oral checking, AI-suggested alignments a person confirms, and AI ranking of
check items. AI assists human judgment. It never edits the translation by itself, never makes the
final quality claim, and is never required.

## Success

Translators stop switching among tools for their core Bible or OBS work. From one project they
start or import work, understand, translate and check it, keep working offline, exchange it with
their team without loss, and produce readable deliverables in their own writing system.
