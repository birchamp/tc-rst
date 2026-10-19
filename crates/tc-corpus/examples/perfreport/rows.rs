//! Every budget unit P measures (PERFORMANCE.md "Budgets"), in the order the table prints
//! them. A row no case filled prints as "not measured"; a budget owned by another unit prints
//! as "not built here".

use crate::table::Class::{Count, Disk, Memory, Mixed, OpenCache, OpenNoCache, Sha, Single, Timer};
use crate::table::Limit::{Count as CountLimit, Info, MiB, Ms, NotBuilt};
use crate::table::Stat::{Max, Median, P95, Value};
use crate::table::{Class, Limit, RowSpec, Stat};

#[allow(clippy::too_many_arguments)]
const fn row(
    id: &'static str,
    area: &'static str,
    action: &'static str,
    scale: &'static str,
    owner: &'static str,
    limit: Limit,
    on: Stat,
    class: Class,
    ci: bool,
) -> RowSpec {
    RowSpec {
        id,
        area,
        action,
        scale,
        owner,
        limit,
        on,
        class,
        ci,
    }
}

const BIBLE: &str = "whole Bible";
const PSALMS: &str = "Psalms in the whole Bible";
const BYTES: Limit = Limit::BytesPerChange(300.0);
const BYTES_DEFAULT: Limit = Limit::BytesPerChange(350.0);

// One row per line: rustfmt would spread each over eleven.
#[rustfmt::skip]
pub const ROWS: &[RowSpec] = &[
    // tc-store: open
    row("open.cache", "Project", "Cold open with a usable fold cache", BIBLE, "tc-store", Ms(3000.0), P95, OpenCache, true),
    row("open.nocache", "Project", "Open with no usable fold cache", BIBLE, "tc-store", Ms(10000.0), P95, OpenNoCache, true),
    row("open.cold", "Project", "First open after a restart, cold disk cache (fold cache kept)", BIBLE, "tc-store", Ms(10000.0), P95, OpenCache, true),
    row("open.progress", "Project", "First progress report of an open", BIBLE, "tc-store", Ms(300.0), P95, Timer, true),
    // tc-journal: the editor
    row("book.open", "Editor", "Open a book (project already open)", PSALMS, "tc-journal", Ms(150.0), P95, Single, true),
    row("book.first_chapter", "Editor", "First chapter's verses ready", PSALMS, "tc-journal", Ms(50.0), P95, Single, true),
    row("chapter.move", "Editor", "Move to another chapter", PSALMS, "tc-journal", Ms(16.0), P95, Single, true),
    row("save.fold_step", "Editor", "Fold update and chapter projection after a save", PSALMS, "tc-journal", Ms(2.0), P95, Single, true),
    row("counts.incremental", "Editor", "Project-wide counts after a save (incremental)", BIBLE, "tc-journal", Ms(1.0), P95, Single, true),
    row("conflicts.scratch", "Editor", "Conflict list and counts from scratch", BIBLE, "tc-journal", Ms(100.0), P95, Single, true),
    // tc-store: saves
    row("save.verse", "Editor", "Save one verse until Saved (fsync included)", PSALMS, "tc-store", Ms(25.0), P95, Disk, true),
    row("save.decision", "Checking", "Record a decision until Saved", PSALMS, "tc-store", Ms(25.0), P95, Disk, true),
    row("save.alignment", "Aligning", "Save an alignment until Saved", PSALMS, "tc-store", Ms(25.0), P95, Disk, true),
    // tc-store: derived files, checkpoint, close
    row("materialize.book_first_edit", "Project", "Materialize Psalms after the first edit of an aligned verse (text and alignments)", PSALMS, "tc-store", Ms(1000.0), P95, Mixed, true),
    row("materialize.book", "Project", "Materialize Psalms after a second edit of the verse (text)", PSALMS, "tc-store", Ms(1000.0), P95, Mixed, false),
    row("materialize.book_alignment", "Project", "Materialize Psalms after an alignment save", PSALMS, "tc-store", Ms(1000.0), P95, Mixed, false),
    row("materialize.book_decision", "Project", "Materialize Psalms after a decision", PSALMS, "tc-store", Ms(1000.0), P95, Mixed, false),
    row("materialize.jude", "Project", "Materialize Jude after a verse save", PSALMS, "tc-store", Ms(1000.0), P95, Mixed, false),
    row("materialize.whole", "Project", "Whole-project materialize (a pins change)", BIBLE, "tc-store", Info, Median, Mixed, true),
    row("materialize.whole_progress", "Project", "First progress report of a whole-project materialize", BIBLE, "tc-store", Ms(300.0), P95, Timer, true),
    row("checkpoint.dirty", "Project", "Checkpoint, Psalms dirty", BIBLE, "tc-store", Ms(500.0), P95, Mixed, true),
    row("checkpoint.settled", "Project", "Checkpoint, materialization already done", BIBLE, "tc-store", Ms(500.0), P95, Mixed, false),
    row("close", "Project", "Close a project, Psalms changed", BIBLE, "tc-store", Ms(3000.0), P95, Mixed, true),
    row("flush", "Project", "One 1 KiB atomic write with its folder flush", "this drive", "tc-store", Info, Median, Disk, true),
    // tc-store: reads and saves while background work runs
    row("r9.materialize", "Project", "Save during a book's materialize", PSALMS, "tc-store", Ms(25.0), P95, Disk, true),
    row("r9.whole", "Project", "Save during a whole-project materialize", PSALMS, "tc-store", Ms(25.0), P95, Disk, true),
    row("r9.checkpoint", "Project", "Save during a checkpoint", PSALMS, "tc-store", Ms(25.0), P95, Disk, true),
    row("r9.repack", "Project", "Save during a repack", PSALMS, "tc-store", Ms(25.0), P95, Disk, false),
    row("r9.consolidation", "Project", "Save during the month's consolidation", PSALMS, "tc-store", Ms(25.0), P95, Disk, false),
    row("r9.upgrade", "Project", "Save during a 150 MB-class ApplyUpgrade", PSALMS, "tc-store", Ms(25.0), P95, Disk, false),
    row("r9.upgrade_materialize", "Project", "Save during the whole-project materialize an upgrade causes", PSALMS, "tc-store", Ms(25.0), P95, Disk, false),
    row("r9.merge_proxy", "Project", "Save during the first checkpoint after a whole-project materialize (merge-commit proxy)", PSALMS, "tc-store", Ms(25.0), P95, Disk, false),
    row("r9.j8", "Project", "Save in a session with a checkpoint at every mode switch (tC4's J8)", PSALMS, "tc-store", Ms(25.0), P95, Disk, false),
    row("r9.j8_none", "Project", "The same session with no checkpoints", PSALMS, "tc-store", Ms(25.0), P95, Disk, false),
    row("read.book_during_materialize", "Editor", "Open a book while a book materializes", PSALMS, "tc-store", Ms(150.0), P95, Single, false),
    row("read.chapter_during_materialize", "Editor", "Move chapter while a book materializes", PSALMS, "tc-store", Ms(16.0), P95, Single, false),
    row("read.book_during_whole", "Editor", "Open a book during a whole-project materialize", PSALMS, "tc-store", Ms(150.0), P95, Single, false),
    row("read.chapter_during_whole", "Editor", "Move chapter during a whole-project materialize", PSALMS, "tc-store", Ms(16.0), P95, Single, false),
    // tc-store: memory
    row("mem.steady", "Project", "Memory of the project store, a minute after the last action (one heap)", PSALMS, "tc-store", MiB(400.0), Value, Memory, false),
    row("mem.steady_default_heap", "Project", "The same with the default heap", PSALMS, "tc-store", MiB(400.0), Value, Memory, false),
    row("mem.peak", "Project", "Peak private bytes, the close and the first open without a cache included (one heap)", PSALMS, "tc-store", MiB(450.0), Value, Memory, false),
    row("mem.peak_default_heap", "Project", "The same with the default heap", PSALMS, "tc-store", MiB(450.0), Value, Memory, false),
    row("mem.growth", "Project", "Memory growth with history (one heap), bytes per added change", "1.2 million changes", "tc-store", BYTES, Value, Memory, false),
    row("mem.growth_default_heap", "Project", "The same with the default heap", "1.2 million changes", "tc-store", BYTES_DEFAULT, Value, Memory, false),
    // three years
    row("years.save", "Project", "Save p95 over months 31-36 against months 4-9", "36 months", "tc-store", Info, Value, Single, false),
    // tc-check
    row("check.open_first", "Checking", "Open the Check screen, first time (derive, map, place, status)", "Psalms tN and tW", "tc-check", Ms(250.0), P95, Single, true),
    row("check.open_cached", "Checking", "Open the Check screen from the cache", "Psalms tN and tW", "tc-check", Ms(20.0), P95, Single, true),
    row("check.rail", "Checking", "Rail filter, sort or grouping", "Psalms", "tc-check", Ms(16.0), P95, Single, true),
    row("check.decision_counts", "Checking", "Progress counts after a decision", "Psalms", "tc-check", Ms(1.0), P95, Single, true),
    row("check.reassess", "Checking", "Re-assess one verse's checks after an edit", "one verse", "tc-check", Ms(1.0), P95, Single, true),
    row("check.consequences", "Checking", "Count what an upgrade or gateway change invalidates", "215,767 checks", "tc-check", Ms(2000.0), P95, Single, true),
    row("check.carry_book", "Checking", "Plan the carry-over of one book", "Psalms", "tc-check", Ms(300.0), P95, Single, true),
    row("check.carry_bible", "Checking", "Plan the carry-over, whole Bible, in the background", BIBLE, "tc-check", Ms(5000.0), P95, Single, false),
    // tc-align
    row("align.open", "Aligning", "Open the aligner on the longest Psalms verse", "PSA 42:4", "tc-align", Ms(30.0), P95, Single, true),
    row("align.edits", "Aligning", "Every aligner edit of that verse", "PSA 42:4", "tc-align", Ms(30.0), P95, Single, true),
    row("align.reflow", "Aligning", "Reflow alignments after a verse edit", "one verse", "tc-align", Ms(2.0), P95, Single, true),
    row("align.suggest", "Aligning", "Suggestions for one verse", "NT + GEN + PSA of ULT v91", "tc-align", Ms(50.0), P95, Single, true),
    row("align.train_ot", "Aligning", "Training the OT model (background)", "GEN + PSA", "tc-align", Info, Median, Single, false),
    row("align.train_nt", "Aligning", "Training the NT model (background)", "27 NT books", "tc-align", Info, Median, Single, false),
    row("align.append", "Aligning", "Model update after an alignment save (background)", "one verse", "tc-align", Info, Median, Single, false),
    // tc-resources
    row("res.load_psalms", "Resources", "Load Psalms resources from the installed store, first time", "Psalms", "tc-resources", Ms(300.0), P95, Single, true),
    row("res.memory_psalms", "Resources", "Memory of one open book's resources", "Psalms", "tc-resources", MiB(60.0), Value, Memory, false),
    row("lex.load", "Resources", "Load a lexicon", "en_uhl, 8,674 entries", "tc-resources", Ms(100.0), P95, Single, true),
    row("lex.lookup", "Resources", "Look an entry up", "en_uhl", "tc-resources", Ms(1.0), P95, Single, true),
    row("seed.first", "App", "First launch: seeding the bundled store (the seeding part of the 60 s gate)", "bundle/en", "tc-resources", Ms(60000.0), Max, Mixed, true),
    row("seed.repeat_reads", "App", "Repeat launch: resource content files read or written", "bundle/en", "tc-resources", CountLimit(0), Max, Count, true),
    row("seed.counts", "App", "First launch: files opened twice, written twice or read back", "bundle/en", "tc-resources", CountLimit(0), Max, Count, true),
    // not this unit's
    row("sync.daily", "Project", "Daily sync", BIBLE, "tc-sync", NotBuilt("unit X"), P95, Sha, false),
    row("export.usb", "Project", "Save work for a teammate (USB export)", BIBLE, "tc-sync", NotBuilt("unit X"), P95, Sha, false),
    row("import.tc4", "Project", "Import a tC4 project", BIBLE, "tc-import", NotBuilt("unit I"), P95, Sha, false),
    row("app.memory", "App", "Whole app memory, 1 GiB steady", BIBLE, "tc-app", NotBuilt("tc-app and the shell"), Value, Memory, false),
];
