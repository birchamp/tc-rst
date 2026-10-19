//! Three years of a team's work, simulated: devices add their books, draft, align and
//! check, make comments, work offline, exchange through a hub at the end of most days, and
//! now and then change a record a teammate changed the same day. Every change names the
//! heads its device's own view showed (I4), so concurrent edits make real conflicts, and
//! devices resolve some of the conflicts they see.

use crate::domain::{BookInput, CheckRow, CheckTool, Domain, VerseInput};
use crate::rng::Rng;
use crate::{CorpusError, DecisionInput, Spec, StatusInput};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tc_core::{BookId, Testament};
use tc_journal::json::{Json, JsonMap};
use tc_journal::pack::{PackSpec, YearMonth, seal};
use tc_journal::pending::{EncodedPending, PendingHeader, Purpose, encode_pending};
use tc_journal::record::{
    AlignmentGroup, AlignmentValue, BookPresence, ChapterFront, CheckValue, CommentTarget,
    CommentValue, DecisionRecord, DeviceProfileValue, Door43Username, Judgement, MarkerWs,
    PinGroupValue, PresentHow, ResourceRef, Scope, Selection, SlotValue, SourceWord, TargetWord,
    UserStatus, UsfmBytes, VerseValue, VersificationValue,
};
use tc_journal::{
    BatchId, BatchRef, Cause, Change, ChangeBody, ChangeId, ChapterNo, CheckId, CheckKey,
    CheckTarget, CommentId, DeviceId, DeviceSeq, GitOid, Md5, MetaField, MetaPath, PackId,
    PinGroup, ProjectId, Record, RecordKey, RefPart, SettingKey, SlotName, ToolId, Verified,
    VerseNo, WallTime,
};

/// How the journal is labelled in [`crate::Corpus::describe`].
pub const HISTORY_ORIGIN: &str = "generated: the history of three years of team work (every change, pack and pending file), simulated from the plain inputs";

/// 2023-01-02T08:00:00Z, a Monday: the first working day.
pub const START_MS: i64 = 1_672_646_400_000;
const DAY_MS: i64 = 86_400_000;

/// The UTC time a working day starts: five working days a week.
fn day_start(day: i64) -> i64 {
    START_MS + (day / 5 * 7 + day % 5) * DAY_MS
}

/// A record to write, the nodes its device's view shows as its heads, and the words and
/// layout of a verse text value.
type Item = (Record, Vec<u32>, Option<Arc<(String, String)>>);

/// A sealed pack, as it would be on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackFile {
    /// `ingredients/tc/changes/<sealer>/<yyyy-mm>/<pack32>.ndjson`.
    pub path: String,
    pub id: PackId,
    pub bytes: Vec<u8>,
    pub changes: usize,
}

/// The changes the simulated team wrote, as device 0's project folder holds them after the
/// last exchange: every device's sealed packs (one per device for each completed month,
/// one per working day in the current month), and device 0's saves of the last day as
/// pending files.
#[derive(Debug, Clone)]
pub struct Journal {
    pub project: ProjectId,
    /// Device 0 is the folder's own device.
    pub devices: Vec<DeviceId>,
    pub packs: Vec<PackFile>,
    pub pending: Vec<EncodedPending>,
    pub stats: Stats,
}

impl Journal {
    /// Every change line of every pack and pending file, as a reader verifies them.
    pub fn lines(&self) -> Vec<Verified> {
        let mut out = Vec::with_capacity(self.stats.changes);
        for pack in &self.packs {
            out.extend(tc_journal::pack::read_pack(&pack.path, &pack.bytes, self.project).entries);
        }
        for file in &self.pending {
            out.extend(tc_journal::pending::read_pending(&file.bytes, self.project).entries);
        }
        out
    }
}

/// What the simulation counted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stats {
    pub changes: usize,
    pub by_kind: BTreeMap<&'static str, usize>,
    pub batches: usize,
    pub section_saves: usize,
    /// Edits of a record a teammate changed that day, made before the two exchanged.
    pub concurrent_edits: usize,
    /// Records some device saw with two or more heads after an exchange.
    pub conflicts_seen: usize,
    pub resolutions: usize,
    pub bytes: usize,
}

pub(crate) struct Output {
    pub journal: Journal,
    pub decisions: Vec<DecisionInput>,
}

struct Node {
    /// Set when the change is encoded ([`Sim::encode`]); its parents' ids come first.
    id: Option<ChangeId>,
    key: u32,
    /// The device that wrote it.
    device: u8,
    parents: smallvec_lite::Parents,
    at: i64,
    known: u32,
    /// The canonical bytes, once encoded.
    raw: Option<Arc<[u8]>>,
    /// The words and layout of a verse `Text` value, for edits that start from it.
    text: Option<Arc<(String, String)>>,
    /// The record, until the change is encoded, and after it for decisions, which later
    /// saves start from.
    record: Option<Arc<Record>>,
    /// What else the change holds, until it is encoded.
    pending: Option<Box<(DeviceSeq, BatchRef, Cause)>>,
    /// The encoded change, until a pack or pending file holds it.
    change: Option<Change>,
}

mod smallvec_lite {
    /// Parent node indices; most changes have none or one.
    #[derive(Debug, Clone, Default)]
    pub struct Parents(pub Vec<u32>);

    impl Parents {
        pub fn from_vec(v: Vec<u32>) -> Self {
            Self(v)
        }

        pub fn contains(&self, n: u32) -> bool {
            self.0.contains(&n)
        }
    }
}

struct Dev {
    id: DeviceId,
    seq: u64,
    cursor: usize,
    unpushed: Vec<u32>,
    /// Changes not yet in a pack, in the order they were written.
    unsealed: Vec<u32>,
    /// The batches of the last day, for device 0's pending files.
    today_batches: Vec<Vec<u32>>,
    conflicts: Vec<u32>,
    books: Vec<usize>,
    current: usize,
    /// Keys this device changed today, for teammates' concurrent edits.
    today: Vec<u32>,
    budget: Budget,
}

#[derive(Debug, Clone, Copy, Default)]
struct Budget {
    verses: u64,
    alignments: u64,
    decisions: u64,
    comments: u64,
}

impl Budget {
    fn total(&self) -> u64 {
        self.verses + self.alignments + self.decisions + self.comments
    }
}

struct BookState {
    added: bool,
    /// One part per device: the chapters it drafts, aligns and checks.
    parts: Vec<Part>,
}

/// A device's share of a book: a run of chapters. Indices point into the book's verses,
/// alignments and checks.
#[derive(Default)]
struct Part {
    /// The work left in this part.
    budget: Budget,
    texts: Vec<usize>,
    drafted: usize,
    alignments: Vec<usize>,
    aligned: usize,
    checks: Vec<usize>,
    checked: usize,
}

pub(crate) struct Sim<'a> {
    spec: &'a Spec,
    domain: &'a Domain,
    rng: Rng,
    project: ProjectId,
    nodes: Vec<Node>,
    /// The nodes not encoded yet, in order.
    unencoded: Vec<u32>,
    keys: HashMap<RecordKey, u32>,
    key_nodes: Vec<Vec<u32>>,
    key_list: Vec<RecordKey>,
    hub: Vec<u32>,
    devs: Vec<Dev>,
    books: Vec<BookState>,
    /// Where seals go: threads encode and hash the packs while the team works on (each
    /// pack is a SHA-256 pass over its bytes).
    sealer: Option<std::sync::mpsc::SyncSender<SealJob>>,
    /// Seals sent so far, so the packs come back in the order they were sealed.
    seals: usize,
    stats: Stats,
    decisions: BTreeMap<(usize, usize), DecisionInput>,
    pins: Pins,
    day: i64,
    clock: i64,
    /// (book, chapter, verse) → index of the verse whose span holds it.
    verse_at: HashMap<(usize, u16, u16), usize>,
    /// (book, chapter, verse) → index of the alignment.
    alignment_at: HashMap<(usize, u16, u16), usize>,
    /// check key → (book, row).
    check_at: HashMap<CheckKey, (usize, usize)>,
}

struct Pins {
    notes: ResourceRef,
    words: ResourceRef,
    ot: ResourceRef,
    nt: ResourceRef,
}

fn oid(label: &str) -> GitOid {
    let digest = tc_burrito::Md5::of(label.as_bytes()).bytes();
    let more = tc_burrito::Md5::of(format!("{label}#").as_bytes()).bytes();
    let mut out = [0u8; 20];
    out[..16].copy_from_slice(&digest);
    out[16..].copy_from_slice(&more[..4]);
    GitOid::from_bytes(out)
}

/// A seal: its place in sealing order, and the device's lines.
type SealJob = (usize, PackSpec, Vec<Verified>);

/// Threads that encode packs while the simulation goes on.
const SEALERS: usize = 3;

/// Seals the jobs of `queue` until it closes: each job's packs, by its place.
fn seal_jobs(
    queue: &std::sync::Mutex<std::sync::mpsc::Receiver<SealJob>>,
) -> Vec<(usize, Result<Vec<PackFile>, CorpusError>)> {
    let mut out = Vec::new();
    loop {
        let job = match queue.lock() {
            Ok(receiver) => receiver.recv(),
            Err(_) => break,
        };
        let Ok((n, spec, lines)) = job else {
            break;
        };
        let packs = seal(&spec, &lines)
            .map_err(|e| err(e.to_string()))
            .map(|sealed| {
                sealed
                    .packs
                    .into_iter()
                    .map(|pack| PackFile {
                        path: pack.path.to_string(),
                        id: pack.id,
                        changes: pack.changes.len(),
                        bytes: pack.bytes,
                    })
                    .collect()
            });
        out.push((n, packs));
    }
    out
}

fn err(text: impl Into<String>) -> CorpusError {
    CorpusError::Build(text.into())
}

fn nz_chapter(n: u16) -> Result<ChapterNo, CorpusError> {
    ChapterNo::new(n).ok_or_else(|| err("chapter 0"))
}

fn nz_verse(n: u16) -> Result<VerseNo, CorpusError> {
    VerseNo::new(n).ok_or_else(|| err("verse 0"))
}

fn md5_of(text: &str) -> Md5 {
    Md5::from_bytes(tc_burrito::Md5::of(text.as_bytes()).bytes())
}

/// md5(plain_text_v1(words + layout)): what an alignment or decision is made against.
fn verse_md5(words: &str, layout: &str) -> Md5 {
    md5_of(&tc_usfm::plain_text_v1(&format!("{words}{layout}")))
}

fn json_obj(fields: Vec<(&str, Json)>) -> Json {
    Json::Obj(fields.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}

fn ref_json(r: &ResourceRef, version: &str, flavor: &str) -> Json {
    json_obj(vec![
        ("flavor", Json::from(flavor)),
        ("repoPath", Json::from(r.repo_path.as_ref())),
        ("sha", Json::from(r.sha.to_hex())),
        ("version", Json::from(version)),
    ])
}

impl<'a> Sim<'a> {
    pub(crate) fn new(spec: &'a Spec, domain: &'a Domain, rng: &Rng) -> Self {
        let mut rng = rng.fork("history");
        let project = ProjectId::from_bytes({
            let mut b: [u8; 16] = rng.bytes();
            b[6] = (b[6] & 0x0f) | 0x40;
            b[8] = (b[8] & 0x3f) | 0x80;
            b
        });
        let devs = (0..spec.devices)
            .map(|_| Dev {
                id: DeviceId::local(rng.bytes()),
                seq: 0,
                cursor: 0,
                unpushed: Vec::new(),
                unsealed: Vec::new(),
                today_batches: Vec::new(),
                conflicts: Vec::new(),
                books: Vec::new(),
                current: 0,
                today: Vec::new(),
                budget: Budget::default(),
            })
            .collect();
        let mut verse_at = HashMap::new();
        let mut alignment_at = HashMap::new();
        let mut check_at = HashMap::new();
        for (b, book) in domain.books.iter().enumerate() {
            for (i, v) in book.verses.iter().enumerate() {
                for n in v.verse..=v.last {
                    verse_at.insert((b, v.chapter, n), i);
                }
            }
            for (i, a) in book.alignments.iter().enumerate() {
                alignment_at.insert((b, a.chapter, a.verse), i);
            }
            for (i, row) in book.checks.iter().enumerate() {
                if let Ok(key) = Self::check_key(row) {
                    check_at.insert(key, (b, i));
                }
            }
        }
        let commit = |hex: &str| GitOid::parse(hex).unwrap_or_else(|| oid(hex));
        let pins = Pins {
            notes: ResourceRef {
                repo_path: "git.door43.org/unfoldingWord/en_tn".into(),
                sha: commit(crate::fixtures::TN_COMMIT),
            },
            words: ResourceRef {
                repo_path: "git.door43.org/unfoldingWord/en_twl".into(),
                sha: commit(crate::fixtures::TWL_COMMIT),
            },
            ot: ResourceRef {
                repo_path: "git.door43.org/unfoldingWord/hbo_uhb".into(),
                sha: commit(crate::fixtures::UHB_COMMIT),
            },
            nt: ResourceRef {
                repo_path: "git.door43.org/unfoldingWord/el-x-koine_ugnt".into(),
                sha: commit(crate::fixtures::UGNT_COMMIT),
            },
        };
        Self {
            spec,
            domain,
            rng,
            project,
            nodes: Vec::new(),
            unencoded: Vec::new(),
            keys: HashMap::new(),
            key_nodes: Vec::new(),
            key_list: Vec::new(),
            hub: Vec::new(),
            devs,
            books: Vec::new(),
            sealer: None,
            seals: 0,
            stats: Stats::default(),
            decisions: BTreeMap::new(),
            pins,
            day: 0,
            clock: START_MS,
            verse_at,
            alignment_at,
            check_at,
        }
    }

    pub(crate) fn run(mut self) -> Result<Output, CorpusError> {
        let (jobs, queue) = std::sync::mpsc::sync_channel::<SealJob>(SEALERS);
        let queue = std::sync::Mutex::new(queue);
        std::thread::scope(|scope| {
            let workers: Vec<_> = (0..SEALERS)
                .map(|_| scope.spawn(|| seal_jobs(&queue)))
                .collect();
            self.sealer = Some(jobs);
            let simulated = self.simulate();
            // Closing the channel lets the sealing threads finish the queue and stop.
            self.sealer = None;
            let mut sealed = Vec::new();
            for worker in workers {
                sealed.extend(worker.join().map_err(|_| err("a sealing thread failed"))?);
            }
            let pending = simulated?;
            sealed.sort_by_key(|(n, _)| *n);
            let mut packs = Vec::new();
            for (_, result) in sealed {
                packs.extend(result?);
            }
            let mut decisions: Vec<DecisionInput> = self.decisions.into_values().collect();
            decisions.sort_by_key(|d| (d.book, d.check));
            Ok(Output {
                journal: Journal {
                    project: self.project,
                    devices: self.devs.iter().map(|d| d.id).collect(),
                    packs,
                    pending,
                    stats: self.stats,
                },
                decisions,
            })
        })
    }

    /// Three years of team work; the pending files of device 0 at the end.
    fn simulate(&mut self) -> Result<Vec<EncodedPending>, CorpusError> {
        self.assign_books();
        self.create_project()?;
        let days = i64::from(self.spec.days.max(1));
        let last_month = YearMonth::of(WallTime(day_start(days - 1)));
        for day in 0..days {
            self.day = day;
            let last = day + 1 == days;
            let mut order: Vec<usize> = (0..self.devs.len()).collect();
            shuffle(&mut self.rng, &mut order);
            for &d in &order {
                self.devs[d].today.clear();
                self.devs[d].today_batches.clear();
            }
            for &d in &order {
                self.clock = day_start(day) + (d as i64) * 1_800_000;
                if day == 0 && d != 0 {
                    self.profile(d)?;
                }
                self.resolve_seen(d)?;
                self.work_day(d, days - day)?;
                if last && d == 0 {
                    // The folder's own device saved something on the last day, so the
                    // folder holds pending files.
                    for _ in 0..3 {
                        if let Some(b) = self.last_book(0) {
                            self.verse_save(0, b, false)?;
                        }
                    }
                }
            }
            shuffle(&mut self.rng, &mut order);
            for &d in &order {
                if last || self.rng.chance(self.spec.sync_rate) {
                    self.push(d);
                    self.pull(d);
                }
            }
            // Checkpoints: in the current month every working day; in earlier months the
            // month's packs are consolidated into one per device, which is what sealing
            // once at the month's end leaves on disk.
            let month = YearMonth::of(WallTime(day_start(day)));
            let next_month = YearMonth::of(WallTime(day_start(day + 1)));
            let sealed = WallTime(day_start(day) + DAY_MS / 2);
            for d in 0..self.devs.len() {
                if last && d == 0 {
                    // Device 0's saves of the last day stay pending; anything older is
                    // sealed.
                    let today: usize = self.devs[0].today_batches.iter().map(Vec::len).sum();
                    let keep = self.devs[0].unsealed.len().saturating_sub(today);
                    let pending = self.devs[0].unsealed.split_off(keep);
                    self.seal(0, sealed)?;
                    self.devs[0].unsealed = pending;
                    continue;
                }
                if month == last_month || next_month != month || last {
                    self.seal(d, sealed)?;
                }
            }
        }
        // Everyone exchanges once more at the end, so device 0 holds every change.
        for d in 0..self.devs.len() {
            self.push(d);
        }
        for d in 0..self.devs.len() {
            self.pull(d);
        }
        self.encode()?;
        let header = PendingHeader {
            device: self.devs[0].id,
            project: self.project,
            purpose: Purpose::Save,
            reconciles: None,
        };
        let batches = std::mem::take(&mut self.devs[0].today_batches);
        let mut pending = Vec::with_capacity(batches.len());
        for batch in batches {
            let lines: Vec<Verified> = batch
                .into_iter()
                .map(|n| self.take_change(n).map(Verified::from))
                .collect::<Result<_, _>>()?;
            pending.push(encode_pending(&header, &lines).map_err(|e| err(e.to_string()))?);
        }
        Ok(pending)
    }

    /// Seals device `d`'s unsealed changes into packs of at most 64 MiB.
    fn seal(&mut self, d: usize, sealed: WallTime) -> Result<(), CorpusError> {
        let changes = std::mem::take(&mut self.devs[d].unsealed);
        if changes.is_empty() {
            return Ok(());
        }
        self.encode()?;
        let lines: Vec<Verified> = changes
            .into_iter()
            .map(|n| self.take_change(n).map(Verified::from))
            .collect::<Result<_, _>>()?;
        let spec = PackSpec {
            project: self.project,
            sealer: self.devs[d].id,
            sealed,
        };
        let sealer = self
            .sealer
            .as_ref()
            .ok_or_else(|| err("a seal after the sealing threads stopped"))?;
        sealer
            .send((self.seals, spec, lines))
            .map_err(|_| err("the sealing threads stopped"))?;
        self.seals += 1;
        Ok(())
    }

    // -----------------------------------------------------------------------------------
    // Planning
    // -----------------------------------------------------------------------------------

    /// The team works through the books together, in order: every device does an equal
    /// share of each book's drafting, aligning, checking and commenting, so two devices
    /// often work in the same chapter on the same day.
    fn assign_books(&mut self) {
        let devices = self.devs.len().max(1);
        for book in &self.domain.books {
            // Chapters in contiguous runs, one run per device.
            let chapters = book.chapters.len().max(1);
            let part_of = |chapter: u16| {
                (usize::from(chapter).saturating_sub(1) * devices / chapters).min(devices - 1)
            };
            let reserved = |chapter: u16| self.spec.reserved.contains(&(book.book, chapter));
            let mut parts: Vec<Part> = (0..devices).map(|_| Part::default()).collect();
            for (i, v) in book.verses.iter().enumerate() {
                if !reserved(v.chapter) {
                    parts[part_of(v.chapter)].texts.push(i);
                }
            }
            for (i, a) in book.alignments.iter().enumerate() {
                if !reserved(a.chapter) {
                    parts[part_of(a.chapter)].alignments.push(i);
                }
            }
            for (i, c) in book.checks.iter().enumerate() {
                let chapter = c.verse_number().map_or(1, |(chapter, _)| chapter);
                if !reserved(chapter) {
                    parts[part_of(chapter)].checks.push(i);
                }
            }
            for part in &mut parts {
                let verses: usize = part
                    .texts
                    .iter()
                    .map(|&i| {
                        let v = &book.verses[i];
                        usize::from(v.last - v.verse) + 1
                    })
                    .sum();
                let verses = verses as f64;
                part.budget = Budget {
                    verses: (verses * self.spec.verse_saves_per_verse).round() as u64,
                    alignments: (verses * self.spec.alignment_saves_per_verse).round() as u64,
                    decisions: (part.checks.len() as f64 * self.spec.decision_saves_per_check)
                        .round() as u64,
                    comments: (verses * self.spec.comments_per_verse).round() as u64,
                };
            }
            self.books.push(BookState {
                added: false,
                parts,
            });
        }
        for d in 0..self.devs.len() {
            let mut total = Budget::default();
            for book in &self.books {
                let budget = book.parts[d].budget;
                total.verses += budget.verses;
                total.alignments += budget.alignments;
                total.decisions += budget.decisions;
                total.comments += budget.comments;
            }
            self.devs[d].books = (0..self.books.len()).collect();
            self.devs[d].budget = total;
        }
    }

    // -----------------------------------------------------------------------------------
    // Writing changes
    // -----------------------------------------------------------------------------------

    fn key_index(&mut self, key: &RecordKey) -> u32 {
        if let Some(&k) = self.keys.get(key) {
            return k;
        }
        let k = self.key_list.len() as u32;
        self.keys.insert(key.clone(), k);
        self.key_list.push(key.clone());
        self.key_nodes.push(Vec::new());
        k
    }

    /// The heads of key `k` in device `d`'s view.
    fn heads(&self, k: u32, d: usize) -> Vec<u32> {
        let bit = 1u32 << d;
        let known: Vec<u32> = self.key_nodes[k as usize]
            .iter()
            .copied()
            .filter(|&n| self.nodes[n as usize].known & bit != 0)
            .collect();
        known
            .iter()
            .copied()
            .filter(|&n| {
                !known
                    .iter()
                    .any(|&m| self.nodes[m as usize].parents.contains(n))
            })
            .collect()
    }

    /// The head a screen shows: the only head, or the newest of several (by `at`, then by
    /// when the simulation wrote it; the app breaks a tie of `at` by id, and the clocks of
    /// the simulated devices never tie).
    fn shown(&self, heads: &[u32]) -> Option<u32> {
        heads
            .iter()
            .copied()
            .max_by_key(|&n| (self.nodes[n as usize].at, n))
    }

    /// The token a screen holds for a record: every head of a settled record, the shown
    /// head of one with several.
    fn token(&self, k: u32, d: usize) -> Vec<u32> {
        let heads = self.heads(k, d);
        if heads.len() <= 1 {
            heads
        } else {
            self.shown(&heads).into_iter().collect()
        }
    }

    /// Writes one batch of device `d`: each record with the parents given.
    fn commit(
        &mut self,
        d: usize,
        items: Vec<Item>,
        cause: &Cause,
    ) -> Result<Vec<u32>, CorpusError> {
        let n = u32::try_from(items.len()).map_err(|_| err("batch too large"))?;
        let batch_id = BatchId::from_bytes(self.rng.bytes());
        let batch = BatchRef::new(batch_id, n).ok_or_else(|| err("empty batch"))?;
        self.stats.batches += 1;
        let mut out = Vec::with_capacity(items.len());
        for (record, mut parents, text) in items {
            self.devs[d].seq += 1;
            self.clock += 1_000 + self.rng.below(20_000) as i64;
            parents.sort_unstable();
            parents.dedup();
            let kind = record_kind(&record);
            let k = self.key_index(&record.key());
            let index = self.nodes.len() as u32;
            self.nodes.push(Node {
                id: None,
                key: k,
                device: d as u8,
                parents: smallvec_lite::Parents::from_vec(parents),
                at: self.clock,
                known: 1 << d,
                raw: None,
                text,
                record: Some(Arc::new(record)),
                pending: Some(Box::new((
                    DeviceSeq(self.devs[d].seq),
                    batch,
                    cause.clone(),
                ))),
                change: None,
            });
            self.key_nodes[k as usize].push(index);
            self.unencoded.push(index);
            self.devs[d].unpushed.push(index);
            self.devs[d].today.push(k);
            self.stats.changes += 1;
            *self.stats.by_kind.entry(kind).or_default() += 1;
            out.push(index);
        }
        self.devs[d].unsealed.extend(out.iter().copied());
        self.devs[d].today_batches.push(out.clone());
        Ok(out)
    }

    /// Encodes every change written since the last call: its canonical bytes and id, which
    /// its children name. The changes of one record form a chain, and records are
    /// independent, so the records are split over threads; this is most of the cost of a
    /// corpus.
    fn encode(&mut self) -> Result<(), CorpusError> {
        let mut by_key: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        for n in std::mem::take(&mut self.unencoded) {
            by_key
                .entry(self.nodes[n as usize].key)
                .or_default()
                .push(n);
        }
        if by_key.is_empty() {
            return Ok(());
        }
        let groups: Vec<Vec<u32>> = by_key.into_values().collect();
        let threads = std::thread::available_parallelism()
            .map_or(1, std::num::NonZeroUsize::get)
            .clamp(1, 8);
        let nodes = &self.nodes;
        let devices: Vec<DeviceId> = self.devs.iter().map(|d| d.id).collect();
        let devices = &devices;
        let groups = &groups;
        let results: Vec<Result<Vec<(u32, Change)>, CorpusError>> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..threads)
                .map(|t| {
                    scope.spawn(move || {
                        let mut out = Vec::new();
                        for group in groups.iter().skip(t).step_by(threads) {
                            let mut fresh: HashMap<u32, ChangeId> = HashMap::new();
                            for &n in group {
                                let node = &nodes[n as usize];
                                let (Some(record), Some(pending)) = (&node.record, &node.pending)
                                else {
                                    return Err(err("a change to encode lost its record"));
                                };
                                let mut parents = Vec::with_capacity(node.parents.0.len());
                                for &p in &node.parents.0 {
                                    let id = nodes[p as usize]
                                        .id
                                        .or_else(|| fresh.get(&p).copied())
                                        .ok_or_else(|| {
                                            err("a parent on another record is not encoded")
                                        })?;
                                    parents.push(id);
                                }
                                parents.sort();
                                let (seq, batch, cause) = &**pending;
                                let body = ChangeBody {
                                    v: 0,
                                    record: (**record).clone(),
                                    parents,
                                    device: devices[usize::from(node.device)],
                                    seq: *seq,
                                    at: WallTime(node.at),
                                    batch: *batch,
                                    cause: cause.clone(),
                                };
                                let kind = record_kind(record);
                                let change =
                                    Change::new(body).map_err(|e| err(format!("{kind}: {e}")))?;
                                fresh.insert(n, change.id());
                                out.push((n, change));
                            }
                        }
                        Ok(out)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join()
                        .unwrap_or_else(|_| Err(err("an encoding thread failed")))
                })
                .collect()
        });
        for result in results {
            for (n, change) in result? {
                let node = &mut self.nodes[n as usize];
                node.id = Some(change.id());
                node.raw = Some(change.raw().clone());
                node.pending = None;
                if !matches!(
                    node.record.as_deref(),
                    Some(Record::Check {
                        value: CheckValue::Decision(_),
                        ..
                    })
                ) {
                    node.record = None;
                }
                self.stats.bytes += change.raw().len();
                node.change = Some(change);
            }
        }
        Ok(())
    }

    /// The encoded change of node `n`, which a pack or pending file now holds.
    fn take_change(&mut self, n: u32) -> Result<Change, CorpusError> {
        self.nodes[n as usize]
            .change
            .take()
            .ok_or_else(|| err("a change was sealed twice or never encoded"))
    }

    fn record_of(&self, n: u32) -> Result<Record, CorpusError> {
        let node = &self.nodes[n as usize];
        if let Some(record) = &node.record {
            return Ok((**record).clone());
        }
        let (Some(id), Some(raw)) = (node.id, &node.raw) else {
            return Err(err("a change has neither its record nor its bytes"));
        };
        match tc_journal::parse_change(id, raw.clone()) {
            tc_journal::Verified::Change(c) => Ok(c.into_body().record),
            tc_journal::Verified::Quarantined(q) => Err(err(format!("{}", q.rule))),
        }
    }

    // -----------------------------------------------------------------------------------
    // Exchange
    // -----------------------------------------------------------------------------------

    fn push(&mut self, d: usize) {
        let hub_bit = 1u32 << 31;
        for n in std::mem::take(&mut self.devs[d].unpushed) {
            let node = &mut self.nodes[n as usize];
            if node.known & hub_bit == 0 {
                node.known |= hub_bit;
                self.hub.push(n);
            }
        }
    }

    fn pull(&mut self, d: usize) {
        let bit = 1u32 << d;
        let from = self.devs[d].cursor;
        let mut touched = Vec::new();
        for i in from..self.hub.len() {
            let n = self.hub[i];
            let node = &mut self.nodes[n as usize];
            if node.known & bit == 0 {
                node.known |= bit;
                touched.push(node.key);
            }
        }
        self.devs[d].cursor = self.hub.len();
        touched.sort_unstable();
        touched.dedup();
        for k in touched {
            if self.heads(k, d).len() > 1 {
                self.stats.conflicts_seen += 1;
                self.devs[d].conflicts.push(k);
            }
        }
    }

    // -----------------------------------------------------------------------------------
    // Actions
    // -----------------------------------------------------------------------------------

    fn create_project(&mut self) -> Result<(), CorpusError> {
        self.clock = START_MS - 3_600_000;
        let created = "2023-01-02T07:00:00.000Z";
        let mut items: Vec<Item> = Vec::new();
        let books: Vec<Json> = self
            .domain
            .books
            .iter()
            .map(|b| Json::from(b.book.code()))
            .collect();
        let with_books = |mut j: Json| {
            if let Json::Obj(map) = &mut j {
                map.insert("books".into(), Json::Arr(books.clone()));
            }
            j
        };
        let slot = |name: &str| SlotName::parse(name).map_err(|e| err(e.to_string()));
        let mut primary = PinGroupValue::default();
        primary.0.insert(
            slot("gatewayLanguage")?,
            SlotValue(json_obj(vec![
                ("languageId", Json::from("en")),
                ("owner", Json::from("unfoldingWord")),
            ])),
        );
        primary.0.insert(
            slot("translationNotes")?,
            SlotValue(with_books(ref_json(
                &self.pins.notes,
                "v91",
                "parascriptural/x-bcvnotes",
            ))),
        );
        primary.0.insert(
            slot("translationWordsLinks")?,
            SlotValue(with_books(ref_json(
                &self.pins.words,
                "v91",
                "parascriptural/x-bcvarticles",
            ))),
        );
        let mut original = PinGroupValue::default();
        original.0.insert(
            slot("ot")?,
            SlotValue(ref_json(
                &self.pins.ot,
                "v3.0.0",
                "scripture/textTranslation",
            )),
        );
        original.0.insert(
            slot("nt")?,
            SlotValue(ref_json(
                &self.pins.nt,
                "v0.34",
                "scripture/textTranslation",
            )),
        );
        items.push((
            Record::Pins {
                group: PinGroup::Primary,
                value: primary,
            },
            vec![],
            None,
        ));
        items.push((
            Record::Pins {
                group: PinGroup::OriginalLanguage,
                value: original,
            },
            vec![],
            None,
        ));
        let categories = json_obj(vec![
            ("translationNotes", Json::Arr(vec![Json::from("translate")])),
            (
                "translationWords",
                Json::Arr(vec![
                    Json::from("kt"),
                    Json::from("names"),
                    Json::from("other"),
                ]),
            ),
        ]);
        items.push((
            Record::Setting {
                key: SettingKey::parse("checkCategories").map_err(|e| err(e.to_string()))?,
                value: Some(categories),
            },
            vec![],
            None,
        ));
        let meta = |field: MetaField, value: Json| {
            (
                Record::ProjectMeta {
                    field,
                    value: Some(value),
                },
                vec![],
                None,
            )
        };
        let extra = |path: &str| -> Result<MetaField, CorpusError> {
            Ok(MetaField::Extra(
                MetaPath::parse(path).map_err(|e| err(e.to_string()))?,
            ))
        };
        items.push(meta(
            MetaField::Name,
            json_obj(vec![("en", Json::from("Scale corpus translation"))]),
        ));
        items.push(meta(
            MetaField::Abbreviation,
            json_obj(vec![("en", Json::from("corpus"))]),
        ));
        items.push(meta(
            MetaField::Languages,
            Json::Arr(vec![json_obj(vec![
                ("name", json_obj(vec![("en", Json::from("English"))])),
                ("scriptDirection", Json::from("ltr")),
                ("tag", Json::from("en")),
            ])]),
        ));
        items.push(meta(extra("meta.dateCreated")?, Json::from(created)));
        items.push(meta(extra("meta.defaultLocale")?, Json::from("en")));
        items.push((
            Record::Versification {
                value: VersificationValue {
                    name: Some("eng".into()),
                    bytes: tc_versification::StandardScheme::Eng.json().to_owned(),
                },
            },
            vec![],
            None,
        ));
        items.push((
            Record::DeviceProfile {
                device: self.devs[0].id,
                value: profile_value(0),
            },
            vec![],
            None,
        ));
        self.commit(0, items, &Cause::Seed)?;
        Ok(())
    }

    fn profile(&mut self, d: usize) -> Result<(), CorpusError> {
        let record = Record::DeviceProfile {
            device: self.devs[d].id,
            value: profile_value(d),
        };
        self.commit(d, vec![(record, vec![], None)], &Cause::Edit)?;
        Ok(())
    }

    fn add_book(&mut self, d: usize, b: usize) -> Result<(), CorpusError> {
        let book: &BookInput = &self.domain.books[b];
        let id = book.book;
        let mut items: Vec<Item> = Vec::new();
        items.push((
            Record::BookPresence {
                book: id,
                value: BookPresence::Present {
                    scope: Scope::Whole,
                    how: PresentHow::Added,
                },
            },
            vec![],
            None,
        ));
        items.push((
            Record::BookHeader {
                book: id,
                value: UsfmBytes(book.header.clone()),
            },
            vec![],
            None,
        ));
        for (i, front) in book.fronts.iter().enumerate() {
            items.push((
                Record::ChapterFront {
                    book: id,
                    chapter: nz_chapter(i as u16 + 1)?,
                    value: ChapterFront::Present(UsfmBytes(front.clone())),
                },
                vec![],
                None,
            ));
        }
        for v in &book.verses {
            let text = Arc::new(("___".to_owned(), v.layout.clone()));
            items.push((verse_record(v, "___", &v.layout)?, vec![], Some(text)));
            for inner in v.verse + 1..=v.last {
                items.push((
                    Record::Verse {
                        book: id,
                        chapter: nz_chapter(v.chapter)?,
                        verse: nz_verse(inner)?,
                        value: VerseValue::Covered {
                            by: nz_verse(v.verse)?,
                        },
                    },
                    vec![],
                    None,
                ));
            }
        }
        self.commit(d, items, &Cause::Seed)?;
        self.books[b].added = true;
        Ok(())
    }

    fn work_day(&mut self, d: usize, days_left: i64) -> Result<(), CorpusError> {
        let remaining = self.devs[d].budget.total();
        if remaining == 0 {
            return Ok(());
        }
        let per_day = (remaining as f64 / days_left.max(1) as f64).max(0.0);
        // About the same amount every day, a little more or less.
        let jitter = 0.75 + self.rng.below(50) as f64 / 100.0;
        let actions = (per_day * jitter).round() as u64;
        for _ in 0..actions {
            if self.devs[d].budget.total() == 0 {
                break;
            }
            if self.rng.chance(self.spec.concurrent_rate) && self.concurrent_edit(d)? {
                continue;
            }
            if !self.own_action(d)? {
                // Nothing this device can work on today (a book it has not received).
                break;
            }
        }
        Ok(())
    }

    /// The book device `d` works on: its first book with work left, added on first use.
    fn current_book(&mut self, d: usize) -> Result<Option<usize>, CorpusError> {
        while self.devs[d].current < self.devs[d].books.len() {
            let b = self.devs[d].books[self.devs[d].current];
            if self.books[b].parts[d].budget.total() > 0 && !self.books[b].parts[d].texts.is_empty()
            {
                if !self.books[b].added {
                    self.add_book(d, b)?;
                } else if !self.has_book(d, b) {
                    // A teammate added the book today; this device works on it once it
                    // has received it.
                    return Ok(None);
                }
                return Ok(Some(b));
            }
            self.devs[d].current += 1;
        }
        Ok(None)
    }

    /// Whether device `d` holds the change that added book `b`.
    fn has_book(&self, d: usize, b: usize) -> bool {
        let key = RecordKey::BookPresence(self.domain.books[b].book);
        self.keys
            .get(&key)
            .is_some_and(|&k| !self.heads(k, d).is_empty())
    }

    /// The last book device `d` has a part of.
    fn last_book(&self, d: usize) -> Option<usize> {
        (0..self.books.len())
            .rev()
            .find(|&b| self.books[b].added && !self.books[b].parts[d].texts.is_empty())
    }

    /// One action of device `d` in its part of its current book; false when it has
    /// nothing to work on.
    fn own_action(&mut self, d: usize) -> Result<bool, CorpusError> {
        let Some(b) = self.current_book(d)? else {
            return Ok(false);
        };
        let budget = self.books[b].parts[d].budget;
        let pick = self.rng.below(budget.total());
        if pick < budget.verses {
            let section = self.rng.chance(self.spec.section_rate);
            let written = self.verse_save(d, b, section)?;
            for _ in 0..written.max(1) {
                self.take(d, b, |x| &mut x.verses);
            }
        } else if pick < budget.verses + budget.alignments {
            self.take(d, b, |x| &mut x.alignments);
            self.alignment_save(d, b, None)?;
        } else if pick < budget.verses + budget.alignments + budget.decisions {
            self.take(d, b, |x| &mut x.decisions);
            self.decision_save(d, b, None)?;
        } else {
            self.take(d, b, |x| &mut x.comments);
            self.comment(d, b)?;
        }
        Ok(true)
    }

    fn take(&mut self, d: usize, b: usize, field: impl Fn(&mut Budget) -> &mut u64) {
        let part = field(&mut self.books[b].parts[d].budget);
        *part = part.saturating_sub(1);
        let dev = field(&mut self.devs[d].budget);
        *dev = dev.saturating_sub(1);
    }

    /// A verse of book `b` to edit: the next undrafted one, or any drafted one.
    /// A verse of device `d`'s part of book `b` to edit: the next undrafted one, or any
    /// drafted one.
    fn verse_target(&mut self, d: usize, b: usize) -> usize {
        let part = &mut self.books[b].parts[d];
        let count = part.texts.len();
        if part.drafted < count && (part.drafted == 0 || self.rng.chance(0.6)) {
            part.drafted += 1;
            part.texts[part.drafted - 1]
        } else {
            let upto = part.drafted.max(1).min(count);
            part.texts[self.rng.index(upto)]
        }
    }

    fn verse_key(v: &VerseInput) -> Result<RecordKey, CorpusError> {
        Ok(RecordKey::Verse(
            v.book,
            nz_chapter(v.chapter)?,
            nz_verse(v.verse)?,
        ))
    }

    /// New words for a verse: the draft for a stub, else a small edit.
    fn edit_words(&mut self, v: &VerseInput, current: &str) -> String {
        if current == "___" {
            return v.words.clone();
        }
        let mut tokens: Vec<String> = current.split(' ').map(str::to_owned).collect();
        let word = self
            .rng
            .pick(&self.domain.edit_words)
            .cloned()
            .unwrap_or_else(|| "word".into());
        let plain: Vec<usize> = tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.is_empty() && t.chars().all(char::is_alphabetic))
            .map(|(i, _)| i)
            .collect();
        match (self.rng.below(3), plain.is_empty()) {
            (0, false) => {
                let at = plain[self.rng.index(plain.len())];
                tokens[at] = word;
            }
            (1, false) if plain.len() > 3 => {
                let at = plain[self.rng.index(plain.len())];
                tokens.remove(at);
            }
            _ => {
                let at = plain
                    .get(self.rng.index(plain.len().max(1)))
                    .copied()
                    .unwrap_or(0);
                tokens.insert(at, word);
            }
        }
        let out = tokens.join(" ");
        if out.is_empty() { v.words.clone() } else { out }
    }

    /// Saves one verse, or a section of two or three verses, and says how many changes it
    /// wrote.
    fn verse_save(&mut self, d: usize, b: usize, section: bool) -> Result<usize, CorpusError> {
        let first = self.verse_target(d, b);
        let book = &self.domain.books[b];
        let mut targets = vec![first];
        if section {
            let extra = 1 + self.rng.index(2);
            for i in 1..=extra {
                let next = first + i;
                if let (Some(a), Some(n)) = (book.verses.get(first), book.verses.get(next))
                    && a.chapter == n.chapter
                {
                    targets.push(next);
                }
            }
        }
        let mut items = Vec::new();
        for t in targets {
            let v = &self.domain.books[b].verses[t];
            let key = Self::verse_key(v)?;
            let k = self.key_index(&key);
            let token = self.token(k, d);
            let shown = self.shown(&token);
            let (words, layout) = match shown.and_then(|n| self.nodes[n as usize].text.clone()) {
                Some(text) => (text.0.clone(), text.1.clone()),
                None => (v.words.clone(), v.layout.clone()),
            };
            let new_words = self.edit_words(v, &words);
            if new_words == words {
                continue;
            }
            let text = Arc::new((new_words.clone(), layout.clone()));
            items.push((verse_record(v, &new_words, &layout)?, token, Some(text)));
        }
        if items.is_empty() {
            return Ok(0);
        }
        if items.len() > 1 {
            self.stats.section_saves += 1;
        }
        let written = items.len();
        self.commit(d, items, &Cause::Edit)?;
        Ok(written)
    }

    /// The words and layout a device shows for a verse.
    fn shown_text(&self, d: usize, v: &VerseInput) -> Result<(String, String), CorpusError> {
        let key = Self::verse_key(v)?;
        let shown = self
            .keys
            .get(&key)
            .and_then(|&k| self.shown(&self.heads(k, d)))
            .and_then(|n| self.nodes[n as usize].text.clone());
        Ok(match shown {
            Some(text) => (text.0.clone(), text.1.clone()),
            None => ("___".into(), v.layout.clone()),
        })
    }

    fn alignment_save(
        &mut self,
        d: usize,
        b: usize,
        target: Option<usize>,
    ) -> Result<(), CorpusError> {
        let book = &self.domain.books[b];
        if book.alignments.is_empty() {
            return Ok(());
        }
        let index = match target {
            Some(t) => t,
            None => {
                let part = &mut self.books[b].parts[d];
                let count = part.alignments.len();
                if count == 0 {
                    return Ok(());
                }
                if part.aligned < count && self.rng.chance(0.7) {
                    part.aligned += 1;
                    part.alignments[part.aligned - 1]
                } else {
                    part.alignments[self.rng.index(count)]
                }
            }
        };
        let a = &self.domain.books[b].alignments[index];
        let Some(v) = self
            .verse_at
            .get(&(b, a.chapter, a.verse))
            .map(|&i| &self.domain.books[b].verses[i])
        else {
            return Ok(());
        };
        let (words, layout) = self.shown_text(d, v)?;
        let testament_ot = a.book.testament() == Testament::Old;
        let (source, label) = if testament_ot {
            (self.pins.ot.clone(), "dcs::unfoldingWord/hbo_uhb@v3.0.0")
        } else {
            (
                self.pins.nt.clone(),
                "dcs::unfoldingWord/el-x-koine_ugnt@v0.34",
            )
        };
        let value = AlignmentValue {
            span_end: nz_verse(v.last)?,
            alignments: a
                .groups
                .iter()
                .map(|g| AlignmentGroup {
                    top_words: g
                        .sources
                        .iter()
                        .map(|s| SourceWord {
                            word: s.word.clone(),
                            strong: s.strong.clone(),
                            lemma: s.lemma.clone(),
                            morph: s.morph.clone(),
                            occurrence: s.occurrence,
                            occurrences: s.occurrences,
                            extra: JsonMap::new(),
                        })
                        .collect(),
                    bottom_words: g
                        .targets
                        .iter()
                        .map(|t| TargetWord {
                            word: t.word.clone(),
                            occurrence: t.occurrence,
                            occurrences: t.occurrences,
                            extra: JsonMap::new(),
                        })
                        .collect(),
                    extra: JsonMap::new(),
                })
                .collect(),
            word_bank: Vec::new(),
            target_verse_md5: verse_md5(&words, &layout),
            source_version: label.into(),
            source: Some(source),
            done: self.rng.chance(0.8),
            marked_invalid: false,
            dismissed: false,
            extra: JsonMap::new(),
        };
        let record = Record::Alignment {
            book: a.book,
            chapter: nz_chapter(a.chapter)?,
            verse: nz_verse(a.verse)?,
            value,
        };
        let k = self.key_index(&record.key());
        let token = self.token(k, d);
        self.commit(d, vec![(record, token, None)], &Cause::Edit)?;
        Ok(())
    }

    fn check_key(row: &CheckRow) -> Result<CheckKey, CorpusError> {
        check_key(row)
    }

    fn context(row: &CheckRow) -> JsonMap {
        let tool = match row.tool {
            CheckTool::TranslationNotes => "translationNotes",
            CheckTool::TranslationWords => "translationWords",
        };
        let quote = match row.tool {
            CheckTool::TranslationNotes => Json::Arr(
                row.quote
                    .split(' ')
                    .filter(|w| !w.is_empty())
                    .map(|w| {
                        json_obj(vec![
                            ("occurrence", Json::from(1u32)),
                            ("word", Json::from(w)),
                        ])
                    })
                    .collect(),
            ),
            CheckTool::TranslationWords => Json::from(row.quote.as_str()),
        };
        let number = |text: &str| match text.parse::<u32>() {
            Ok(n) => Json::from(n),
            Err(_) => Json::from(text),
        };
        let reference = json_obj(vec![
            ("bookId", Json::from(row.book.code().to_lowercase())),
            ("chapter", number(&row.chapter)),
            ("verse", number(&row.verse)),
        ]);
        let Json::Obj(map) = json_obj(vec![
            ("checkId", Json::from(row.id.as_str())),
            ("glQuote", Json::from("")),
            ("groupId", Json::from(row.group.as_str())),
            ("occurrence", Json::from(row.occurrence)),
            ("occurrenceNote", Json::from(row.note.as_str())),
            ("quote", quote),
            ("quoteString", Json::from(row.quote.as_str())),
            ("reference", reference),
            ("tool", Json::from(tool)),
        ]) else {
            return JsonMap::new();
        };
        map
    }

    fn decision_save(
        &mut self,
        d: usize,
        b: usize,
        target: Option<usize>,
    ) -> Result<(), CorpusError> {
        let book = &self.domain.books[b];
        if book.checks.is_empty() {
            return Ok(());
        }
        let index = match target {
            Some(t) => t,
            None => {
                let part = &mut self.books[b].parts[d];
                let count = part.checks.len();
                if count == 0 {
                    return Ok(());
                }
                if part.checked < count && self.rng.chance(0.77) {
                    part.checked += 1;
                    part.checks[part.checked - 1]
                } else {
                    part.checks[self.rng.index(count)]
                }
            }
        };
        let row = &self.domain.books[b].checks[index];
        let key = Self::check_key(row)?;
        let k = self.key_index(&RecordKey::Check(key.clone()));
        let token = self.token(k, d);
        // A row on a chapter or book introduction is judged against no verse text.
        let verse = row.verse_number().and_then(|(chapter, verse)| {
            self.verse_at
                .get(&(b, chapter, verse))
                .map(|&i| &self.domain.books[b].verses[i])
        });
        let (words, layout) = match verse {
            Some(v) => self.shown_text(d, v)?,
            None => (String::new(), String::new()),
        };
        let md5 = verse_md5(&words, &layout);
        let resource = match row.tool {
            CheckTool::TranslationNotes => self.pins.notes.clone(),
            CheckTool::TranslationWords => self.pins.words.clone(),
        };
        let previous = match self.shown(&token) {
            Some(n) => match self.record_of(n)? {
                Record::Check {
                    value: CheckValue::Decision(previous),
                    ..
                } => Some(previous),
                _ => None,
            },
            None => None,
        };
        let status = match self.rng.below(10) {
            0 => UserStatus::Invalid,
            1 => UserStatus::Todo,
            _ => UserStatus::Valid,
        };
        let decision = match previous {
            // A later visit changes one thing: the status, the bookmark or the comment.
            Some(mut previous) => {
                match self.rng.below(4) {
                    0 => previous.bookmarked = !previous.bookmarked,
                    1 => {
                        previous.comment = Some(format!(
                            "Check this rendering again ({}).",
                            self.rng.below(1000)
                        ))
                    }
                    _ => {
                        previous.judgement.status = Some(status);
                        previous.judgement.verse_text_md5 = md5;
                        previous.imported_invalid = false;
                    }
                }
                previous
            }
            None => {
                let candidates: Vec<&str> = words
                    .split(' ')
                    .filter(|w| !w.is_empty() && w.chars().all(char::is_alphabetic))
                    .collect();
                let selections = self.rng.pick(&candidates).map(|w| {
                    let occurrences = candidates.iter().filter(|c| *c == w).count() as u32;
                    vec![Selection {
                        text: (*w).to_owned(),
                        occurrence: 1,
                        occurrences,
                        extra: JsonMap::new(),
                    }]
                });
                DecisionRecord {
                    context: Self::context(row),
                    category: Some(row.category.as_str().into()),
                    judgement: Judgement {
                        nothing_to_select: selections.is_none(),
                        selections,
                        status: Some(status),
                        verse_text_md5: md5,
                    },
                    comment: None,
                    bookmarked: self.rng.chance(0.03),
                    resource,
                    imported_invalid: false,
                    dismissed: false,
                    extra: JsonMap::new(),
                }
            }
        };
        let selections = decision
            .judgement
            .selections
            .iter()
            .flatten()
            .map(|s| crate::domain::TargetWordInput {
                word: s.text.clone(),
                occurrence: s.occurrence,
                occurrences: s.occurrences,
            })
            .collect();
        self.decisions.insert(
            (b, index),
            DecisionInput {
                book: b,
                check: index,
                status: decision.judgement.status.map(|s| match s {
                    UserStatus::Valid => StatusInput::Valid,
                    UserStatus::Invalid => StatusInput::Invalid,
                    UserStatus::Todo => StatusInput::Todo,
                }),
                selections,
                nothing_to_select: decision.judgement.nothing_to_select,
                comment: decision.comment.clone(),
                bookmarked: decision.bookmarked,
            },
        );
        let record = Record::Check {
            key,
            value: CheckValue::Decision(decision),
        };
        self.commit(d, vec![(record, token, None)], &Cause::Edit)?;
        Ok(())
    }

    fn comment(&mut self, d: usize, b: usize) -> Result<(), CorpusError> {
        let part = &self.books[b].parts[d].texts;
        let Some(&i) = part.get(self.rng.index(part.len())) else {
            return Ok(());
        };
        let v = &self.domain.books[b].verses[i];
        let record = Record::Comment {
            id: CommentId::from_bytes(self.rng.bytes()),
            value: CommentValue {
                target: CommentTarget::Verse {
                    book: v.book,
                    chapter: nz_chapter(v.chapter)?,
                    verse: nz_verse(v.verse)?,
                },
                text: format!(
                    "Should this verse say \"{}\" here? ({})",
                    self.rng
                        .pick(&self.domain.edit_words)
                        .cloned()
                        .unwrap_or_default(),
                    self.rng.below(10_000)
                ),
            },
        };
        self.commit(d, vec![(record, vec![], None)], &Cause::Edit)?;
        Ok(())
    }

    /// Changes a record a teammate changed today, from this device's own (stale) view.
    /// Returns false when no teammate changed anything today.
    fn concurrent_edit(&mut self, d: usize) -> Result<bool, CorpusError> {
        let others: Vec<usize> = (0..self.devs.len())
            .filter(|&o| o != d && !self.devs[o].today.is_empty())
            .collect();
        let Some(&o) = self.rng.pick(&others) else {
            return Ok(false);
        };
        let k = self.devs[o].today[self.rng.index(self.devs[o].today.len())];
        let key = self.key_list[k as usize].clone();
        let done = match &key {
            RecordKey::Verse(book, chapter, verse) => {
                match self.book_and_verse(*book, chapter.get(), verse.get()) {
                    Some((b, t)) => {
                        self.edit_verse_at(d, b, t)?;
                        true
                    }
                    None => false,
                }
            }
            RecordKey::Alignment(book, chapter, verse) => {
                match self.book_index(*book).and_then(|b| {
                    self.alignment_at
                        .get(&(b, chapter.get(), verse.get()))
                        .map(|&i| (b, i))
                }) {
                    Some((b, i)) => {
                        self.alignment_save(d, b, Some(i))?;
                        true
                    }
                    None => false,
                }
            }
            RecordKey::Check(check) => match self.check_at.get(check).copied() {
                Some((b, i)) => {
                    self.decision_save(d, b, Some(i))?;
                    true
                }
                None => false,
            },
            _ => false,
        };
        if done {
            self.stats.concurrent_edits += 1;
        }
        Ok(done)
    }

    fn book_index(&self, book: BookId) -> Option<usize> {
        self.domain.books.iter().position(|b| b.book == book)
    }

    fn book_and_verse(&self, book: BookId, chapter: u16, verse: u16) -> Option<(usize, usize)> {
        let b = self.book_index(book)?;
        let t = *self.verse_at.get(&(b, chapter, verse))?;
        Some((b, t))
    }

    fn edit_verse_at(&mut self, d: usize, b: usize, t: usize) -> Result<(), CorpusError> {
        let v = &self.domain.books[b].verses[t];
        let key = Self::verse_key(v)?;
        let k = self.key_index(&key);
        let token = self.token(k, d);
        let (words, layout) = match self
            .shown(&token)
            .and_then(|n| self.nodes[n as usize].text.clone())
        {
            Some(text) => (text.0.clone(), text.1.clone()),
            None => return Ok(()),
        };
        let new_words = self.edit_words(v, &words);
        if new_words == words {
            return Ok(());
        }
        let text = Arc::new((new_words.clone(), layout.clone()));
        let record = verse_record(v, &new_words, &layout)?;
        self.commit(d, vec![(record, token, Some(text))], &Cause::Edit)?;
        Ok(())
    }

    /// Resolves some of the conflicts device `d` has seen on records it wrote one of the
    /// versions of: keeps one version, naming every head it shows as parents.
    fn resolve_seen(&mut self, d: usize) -> Result<(), CorpusError> {
        let seen = std::mem::take(&mut self.devs[d].conflicts);
        for k in seen {
            let heads = self.heads(k, d);
            if heads.len() < 2
                || !heads
                    .iter()
                    .any(|&h| usize::from(self.nodes[h as usize].device) == d)
            {
                continue;
            }
            if !self.rng.chance(self.spec.resolve_rate) {
                continue;
            }
            let keep = heads[self.rng.index(heads.len())];
            let record = self.record_of(keep)?;
            let text = self.nodes[keep as usize].text.clone();
            self.commit(d, vec![(record, heads, text)], &Cause::Resolve)?;
            self.stats.resolutions += 1;
        }
        Ok(())
    }
}

fn verse_record(v: &VerseInput, words: &str, layout: &str) -> Result<Record, CorpusError> {
    Ok(Record::Verse {
        book: v.book,
        chapter: nz_chapter(v.chapter)?,
        verse: nz_verse(v.verse)?,
        value: VerseValue::Text {
            marker: MarkerWs::canonical(),
            words: words.to_owned(),
            layout: layout.to_owned(),
            last: nz_verse(v.last)?,
            from: None,
        },
    })
}

fn profile_value(d: usize) -> DeviceProfileValue {
    const NAMES: [&str; 8] = [
        "Maria", "Juan", "Ana", "Pedro", "Lucia", "Tomas", "Elena", "Pablo",
    ];
    DeviceProfileValue {
        display_name: Some(NAMES[d % NAMES.len()].into()),
        door43_username: Door43Username::parse(&format!("corpus-translator-{d}")).ok(),
        legacy: None,
    }
}

fn record_kind(record: &Record) -> &'static str {
    match record {
        Record::BookPresence { .. } => "bookPresence",
        Record::BookHeader { .. } => "bookHeader",
        Record::ChapterFront { .. } => "chapterFront",
        Record::Verse { .. } => "verse",
        Record::BookLayout { .. } => "bookLayout",
        Record::Alignment { .. } => "alignment",
        Record::Check { .. } => "check",
        Record::Comment { .. } => "comment",
        Record::Pins { .. } => "pins",
        Record::ResourcesExtra { .. } => "resourcesExtra",
        Record::Setting { .. } => "setting",
        Record::ProjectMeta { .. } => "projectMeta",
        Record::Versification { .. } => "versification",
        Record::DeviceProfile { .. } => "deviceProfile",
        Record::StoryBase { .. } => "storyBase",
        Record::Frame { .. } => "frame",
        Record::StoryRef { .. } => "storyRef",
        Record::CarriedFile { .. } => "carriedFile",
        Record::BatchRelease { .. } => "batchRelease",
        Record::Unknown(_) => "unknown",
    }
}

fn shuffle<T>(rng: &mut Rng, items: &mut [T]) {
    for i in (1..items.len()).rev() {
        let j = rng.index(i + 1);
        items.swap(i, j);
    }
}

/// The journal key of a check row: tC4's decision identity.
pub fn check_key(row: &CheckRow) -> Result<CheckKey, CorpusError> {
    Ok(CheckKey {
        tool: match row.tool {
            CheckTool::TranslationNotes => ToolId::TranslationNotes,
            CheckTool::TranslationWords => ToolId::TranslationWords,
        },
        check_id: CheckId::parse(&row.id).map_err(|e| err(e.to_string()))?,
        target: CheckTarget::Verse {
            book: row.book,
            chapter: RefPart::parse(&row.chapter).map_err(|e| err(e.to_string()))?,
            verse: RefPart::parse(&row.verse).map_err(|e| err(e.to_string()))?,
        },
        occurrence: row.occurrence,
    })
}

/// tC4's `contextId` of a check row: what a decision records about the check it is for.
pub fn check_context(row: &CheckRow) -> JsonMap {
    Sim::context(row)
}
