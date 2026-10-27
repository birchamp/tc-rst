//! Located journal values remain readable across seals and consolidations (§2.6, §5.12).
//!
//! Fold inputs keep their original file number and byte span. When a file is replaced,
//! those original offsets become aliases of stable value identities. Each identity names
//! one current file/span directly, so reading a value never walks consolidation history.
//! Files created only as relocation targets retain no per-value alias map when retired.
//! Verified ChangeIds coalesce duplicate sources that move into the same output pack.
//!
//! Loader handles are attached before their inputs enter the fold. Relocation outputs
//! are opened before publication, outside the routing lock, so reading them needs no
//! filesystem open while holding that lock. An unattached original source acquires its
//! handle under the read lock before relocation can unlink its path. The lock is released
//! before reading bytes; the handle stays alive for the read.

use crate::vfs::{ReadHandle, Vfs};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tc_journal::ChangeId;
use tc_journal::fold::{ValueAt, ValueSource};

/// Both ends were verified and matched by change id by load::relocation.
#[derive(Debug, Clone, Copy)]
pub struct RelocatedValue {
    pub id: ChangeId,
    pub source_len: u32,
    pub destination: ValueAt,
}

#[derive(Debug, Clone, Copy)]
enum Exposure {
    FoldValues,
    RelocationOnly,
}

struct BackingFile {
    path: PathBuf,
    handle: Mutex<Option<Arc<dyn ReadHandle>>>,
}

impl std::fmt::Debug for BackingFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("BackingFile").field(&self.path).finish()
    }
}

impl BackingFile {
    fn open(&self, vfs: &dyn Vfs) -> Option<Arc<dyn ReadHandle>> {
        let mut held = self.handle.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(handle) = &*held {
            return Some(handle.clone());
        }
        let opened: Arc<dyn ReadHandle> = Arc::from(vfs.open(&self.path).ok()?);
        *held = Some(opened.clone());
        Some(opened)
    }
}

#[derive(Debug, Clone, Copy)]
struct ValueId(u32);

#[derive(Debug, Clone, Copy)]
struct Alias {
    value: ValueId,
    len: u32,
}

#[derive(Debug, Clone)]
struct Location {
    backing: Arc<BackingFile>,
    offset: u64,
    len: u32,
}

#[derive(Debug)]
enum Slot {
    File {
        backing: Arc<BackingFile>,
        exposure: Exposure,
    },
    Aliases(HashMap<u64, Alias>),
    Retired,
}

#[derive(Debug, Default)]
struct Locations {
    slots: Vec<Slot>,
    ids: HashMap<ChangeId, ValueId>,
    values: Vec<Location>,
}

#[derive(Debug)]
pub struct FileSource {
    vfs: Arc<dyn Vfs>,
    locations: RwLock<Locations>,
    pub reads: AtomicU64,
    pub bytes: AtomicU64,
    #[cfg(any(test, feature = "test-hooks"))]
    lookup_steps: AtomicU64,
    #[cfg(any(test, feature = "test-hooks"))]
    relocation_steps: AtomicU64,
}

impl FileSource {
    pub fn new(vfs: Arc<dyn Vfs>) -> Self {
        Self {
            vfs,
            locations: RwLock::new(Locations::default()),
            reads: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            #[cfg(any(test, feature = "test-hooks"))]
            lookup_steps: AtomicU64::new(0),
            #[cfg(any(test, feature = "test-hooks"))]
            relocation_steps: AtomicU64::new(0),
        }
    }

    /// Registers a file whose located values may enter the fold.
    pub fn add(&self, path: &Path) -> u32 {
        self.add_with_exposure(path, Exposure::FoldValues, None)
    }

    /// A verified output used only as a relocation destination or index section. Its
    /// handle is acquired before publishing its slot, so a snapshot cannot stall routing
    /// updates on its first open. Its number never enters the running fold's inputs.
    pub fn add_target(&self, path: &Path) -> io::Result<u32> {
        let handle = Arc::from(self.vfs.open(path)?);
        Ok(self.add_with_exposure(path, Exposure::RelocationOnly, Some(handle)))
    }

    /// [`Self::add_target`] for bytes just written and verified, read from memory.
    pub fn add_target_held(&self, path: &Path, handle: Arc<dyn ReadHandle>) -> u32 {
        self.add_with_exposure(path, Exposure::RelocationOnly, Some(handle))
    }

    fn add_with_exposure(
        &self,
        path: &Path,
        exposure: Exposure,
        handle: Option<Arc<dyn ReadHandle>>,
    ) -> u32 {
        let mut locations = self.locations.write().unwrap_or_else(|e| e.into_inner());
        locations.slots.push(Slot::File {
            backing: Arc::new(BackingFile {
                path: path.to_owned(),
                handle: Mutex::new(handle),
            }),
            exposure,
        });
        // Retains the existing API's file-number range; a fallible add is separate work.
        (locations.slots.len() - 1) as u32
    }

    pub fn number_of(&self, path: &Path) -> Option<u32> {
        let locations = self.locations.read().unwrap_or_else(|e| e.into_inner());
        locations
            .slots
            .iter()
            .position(|slot| matches!(slot, Slot::File { backing, .. } if backing.path == path))
            .map(|i| i as u32)
    }

    pub fn attach(&self, file: u32, handle: Arc<dyn ReadHandle>) {
        let locations = self.locations.read().unwrap_or_else(|e| e.into_inner());
        if let Some(Slot::File { backing, .. }) = locations.slots.get(file as usize) {
            let mut held = backing.handle.lock().unwrap_or_else(|e| e.into_inner());
            if held.is_none() {
                *held = Some(handle);
            }
        }
    }

    /// Retargets only the supplied verified identities. False leaves all routing
    /// unchanged; callers must retain the source file then.
    pub fn moved(&self, file: u32, map: HashMap<u64, RelocatedValue>) -> bool {
        let mut locations = self.locations.write().unwrap_or_else(|e| e.into_inner());
        let expose = match locations.slots.get(file as usize) {
            Some(Slot::File {
                exposure: Exposure::FoldValues,
                ..
            }) => true,
            Some(Slot::File {
                exposure: Exposure::RelocationOnly,
                ..
            })
            | Some(Slot::Aliases(_))
            | Some(Slot::Retired) => false,
            None => return false,
        };
        let mut prepared = Vec::with_capacity(map.len());
        let mut new_ids = HashMap::new();
        for (offset, value) in map {
            self.count_relocation_step();
            if value.destination.file == file || value.destination.len != value.source_len {
                return false;
            }
            let Some(Slot::File { backing, .. }) =
                locations.slots.get(value.destination.file as usize)
            else {
                return false;
            };
            let destination = Location {
                backing: backing.clone(),
                offset: value.destination.offset,
                len: value.destination.len,
            };
            if let Some(id) = locations.ids.get(&value.id) {
                if locations
                    .values
                    .get(id.0 as usize)
                    .is_none_or(|prior| prior.len != destination.len)
                {
                    return false;
                }
            } else if expose
                && new_ids
                    .insert(value.id, destination.len)
                    .is_some_and(|prior| prior != destination.len)
            {
                return false;
            }
            prepared.push((offset, value.id, destination));
        }
        if locations
            .values
            .len()
            .checked_add(new_ids.len())
            .is_none_or(|len| len > u32::MAX as usize)
        {
            return false;
        }
        let mut aliases = expose.then(|| HashMap::with_capacity(prepared.len()));
        for (offset, change, destination) in prepared {
            self.count_relocation_step();
            let value = match locations.ids.get(&change).copied() {
                Some(value) => value,
                None if expose => {
                    // The whole input's arena growth was checked above.
                    let value = ValueId(locations.values.len() as u32);
                    locations.values.push(destination.clone());
                    locations.ids.insert(change, value);
                    value
                }
                None => continue,
            };
            let len = destination.len;
            locations.values[value.0 as usize] = destination;
            if let Some(aliases) = &mut aliases {
                aliases.insert(offset, Alias { value, len });
            }
        }
        match locations.slots.get_mut(file as usize) {
            Some(slot @ Slot::File { .. }) => {
                *slot = match aliases {
                    Some(aliases) => Slot::Aliases(aliases),
                    None => Slot::Retired,
                };
            }
            Some(Slot::Aliases(_)) | Some(Slot::Retired) => {}
            None => return false,
        }
        true
    }

    #[inline]
    fn count_lookup_step(&self) {
        #[cfg(any(test, feature = "test-hooks"))]
        self.lookup_steps.fetch_add(1, Ordering::Relaxed);
    }

    #[inline]
    fn count_relocation_step(&self) {
        #[cfg(any(test, feature = "test-hooks"))]
        self.relocation_steps.fetch_add(1, Ordering::Relaxed);
    }

    fn read_located(&self, at: ValueAt) -> Option<Vec<u8>> {
        let (handle, offset, len) = {
            let locations = self.locations.read().unwrap_or_else(|e| e.into_inner());
            self.count_lookup_step();
            let location = match locations.slots.get(at.file as usize)? {
                Slot::File {
                    backing,
                    exposure: Exposure::FoldValues,
                } => Location {
                    backing: backing.clone(),
                    offset: at.offset,
                    len: at.len,
                },
                Slot::Aliases(aliases) => {
                    self.count_lookup_step();
                    let alias = aliases.get(&at.offset)?;
                    if alias.len != at.len {
                        return None;
                    }
                    self.count_lookup_step();
                    let location = locations.values.get(alias.value.0 as usize)?;
                    if location.len != at.len {
                        return None;
                    }
                    location.clone()
                }
                Slot::File {
                    exposure: Exposure::RelocationOnly,
                    ..
                }
                | Slot::Retired => return None,
            };
            // A relocation cannot publish and unlink this path before its chosen
            // reader has acquired the old handle.
            let handle = location.backing.open(&*self.vfs)?;
            (handle, location.offset, location.len)
        };
        let mut bytes = vec![0u8; len as usize];
        handle.read_at(&mut bytes, offset).ok()?;
        self.reads.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(u64::from(len), Ordering::Relaxed);
        Some(bytes)
    }
}

impl ValueSource for FileSource {
    fn read(&self, at: ValueAt) -> Option<Vec<u8>> {
        self.read_located(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fault::FaultFs;
    use std::collections::{BTreeMap, BTreeSet};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum Label {
        A,
        B,
        C,
    }

    impl Label {
        fn bytes(self) -> &'static [u8] {
            match self {
                Self::A => b"first saved words",
                Self::B => b"another saved version",
                Self::C => b"unrelated book's historical words",
            }
        }

        fn id(self) -> ChangeId {
            ChangeId::from_bytes(
                [match self {
                    Self::A => 1,
                    Self::B => 2,
                    Self::C => 3,
                }; 32],
            )
        }
    }

    #[derive(Clone)]
    struct Image {
        file: u32,
        path: PathBuf,
        spans: Vec<(Label, ValueAt)>,
    }

    #[derive(Clone, Copy)]
    enum Registration {
        Exposed,
        Target,
    }

    struct Rig {
        fs: FaultFs,
        source: FileSource,
    }

    impl Rig {
        fn new() -> Self {
            let fs = FaultFs::new();
            fs.create_dir(Path::new("/values")).unwrap();
            let source = FileSource::new(Arc::new(fs.clone()));
            Self { fs, source }
        }

        fn image(
            &self,
            name: &str,
            registration: Registration,
            labels: &[Label],
            pad: usize,
        ) -> Image {
            let path = Path::new("/values").join(name);
            let mut image = self.rewrite(u32::MAX, path.clone(), labels, pad);
            let file = match registration {
                Registration::Exposed => self.source.add(&path),
                Registration::Target => self.source.add_target(&path).unwrap(),
            };
            image.file = file;
            for (_, at) in &mut image.spans {
                at.file = file;
            }
            image
        }

        fn rewrite(&self, file: u32, path: PathBuf, labels: &[Label], pad: usize) -> Image {
            let mut bytes = vec![b'_'; pad];
            let mut spans = Vec::new();
            for label in labels {
                let at = ValueAt {
                    file,
                    offset: bytes.len() as u64,
                    len: label.bytes().len() as u32,
                };
                bytes.extend_from_slice(label.bytes());
                bytes.push(b'|');
                spans.push((*label, at));
            }
            self.fs.put_unseen(&path, &bytes, None);
            Image { file, path, spans }
        }
    }

    fn mapping(from: &Image, to: &Image) -> HashMap<u64, RelocatedValue> {
        from.spans
            .iter()
            .map(|(label, old)| {
                let destination = to.spans.iter().find(|(other, _)| other == label).unwrap().1;
                (
                    old.offset,
                    RelocatedValue {
                        id: label.id(),
                        source_len: old.len,
                        destination,
                    },
                )
            })
            .collect()
    }

    // The oracle preserves immutable logical bytes, not any production placement or
    // routing decision. Its physical inventory only decides whether removal is legal.
    #[derive(Default)]
    struct Reference {
        origins: Vec<(ValueAt, Label)>,
        physical: BTreeMap<u32, BTreeSet<Label>>,
    }

    enum Event<'a> {
        Published(&'a Image),
        Exposed(&'a Image),
        Relocated { from: u32, to: u32 },
        Removed(u32),
    }

    impl Reference {
        fn step(&mut self, event: Event<'_>) {
            match event {
                Event::Published(image) => {
                    self.physical.insert(
                        image.file,
                        image.spans.iter().map(|(label, _)| *label).collect(),
                    );
                }
                Event::Exposed(image) => {
                    self.origins
                        .extend(image.spans.iter().map(|(label, at)| (*at, *label)));
                }
                Event::Relocated { from, to } => {
                    assert!(self.physical[&from].is_subset(&self.physical[&to]));
                }
                Event::Removed(file) => {
                    self.physical.remove(&file).unwrap();
                    for (_, label) in &self.origins {
                        assert!(self.physical.values().any(|labels| labels.contains(label)));
                    }
                }
            }
        }

        fn check(&self, rig: &Rig, expected_lookups: u64) {
            let lookups = rig.source.lookup_steps.load(Ordering::Relaxed);
            let reads = rig.source.reads.load(Ordering::Relaxed);
            let bytes = rig.source.bytes.load(Ordering::Relaxed);
            let disk_bytes = rig.fs.bytes_read();
            for (at, label) in &self.origins {
                assert_eq!(
                    rig.source.read(*at).as_deref(),
                    Some(label.bytes()),
                    "origin {at:?} lost {label:?}"
                );
            }
            let expected_bytes: u64 = self
                .origins
                .iter()
                .map(|(_, label)| label.bytes().len() as u64)
                .sum();
            assert_eq!(
                rig.source.lookup_steps.load(Ordering::Relaxed) - lookups,
                expected_lookups
            );
            assert_eq!(
                rig.source.reads.load(Ordering::Relaxed) - reads,
                self.origins.len() as u64
            );
            assert_eq!(
                rig.source.bytes.load(Ordering::Relaxed) - bytes,
                expected_bytes
            );
            assert_eq!(rig.fs.bytes_read() - disk_bytes, expected_bytes);
        }
    }

    fn routing_sizes(source: &FileSource) -> (usize, usize, usize) {
        let state = source.locations.read().unwrap();
        (
            state.ids.len(),
            state.values.len(),
            state
                .slots
                .iter()
                .map(|slot| match slot {
                    Slot::Aliases(aliases) => aliases.len(),
                    Slot::File { .. } | Slot::Retired => 0,
                })
                .sum(),
        )
    }

    #[test]
    fn bounded_relocations_keep_independent_bytes_and_constant_read_work() {
        // Enumerate merge order, initial byte ordering, and retaining/retrying an
        // original source. Generations exceed the former eight-hop limit in every trace.
        for trace in 0..8 {
            let rig = Rig::new();
            let first = rig.image(
                "first",
                Registration::Exposed,
                &[Label::A, Label::B, Label::A],
                3,
            );
            let duplicate = rig.image("duplicate", Registration::Exposed, &[Label::A], 11);
            let unrelated = rig.image("unrelated", Registration::Exposed, &[Label::C], 5);
            let mut model = Reference::default();
            for image in [&first, &duplicate, &unrelated] {
                model.step(Event::Published(image));
                model.step(Event::Exposed(image));
            }
            model.check(&rig, 5);
            let order = if trace & 2 == 0 {
                [Label::A, Label::B]
            } else {
                [Label::B, Label::A]
            };
            let mut current = rig.image("initial-output", Registration::Target, &order, 17);
            model.step(Event::Published(&current));
            let merge_order = if trace & 1 == 0 {
                [&first, &duplicate]
            } else {
                [&duplicate, &first]
            };
            for old in merge_order {
                model.step(Event::Relocated {
                    from: old.file,
                    to: current.file,
                });
                assert!(rig.source.moved(old.file, mapping(old, &current)));
            }
            rig.fs.remove_file(&duplicate.path).unwrap();
            model.step(Event::Removed(duplicate.file));
            if trace & 4 != 0 {
                // The source remained on disk after a held unlink. Its physical offsets
                // can change; previously exposed offsets still mean their original bytes.
                let rewritten = rig.rewrite(
                    first.file,
                    first.path.clone(),
                    &[Label::B, Label::A, Label::A],
                    83,
                );
                model.step(Event::Published(&rewritten));
                let next = rig.image(
                    "retry-output",
                    Registration::Target,
                    &[Label::A, Label::B],
                    41,
                );
                model.step(Event::Published(&next));
                model.step(Event::Relocated {
                    from: rewritten.file,
                    to: next.file,
                });
                assert!(rig.source.moved(rewritten.file, mapping(&rewritten, &next)));
                rig.fs.remove_file(&current.path).unwrap();
                model.step(Event::Removed(current.file));
                current = next;
            }
            rig.fs.remove_file(&first.path).unwrap();
            model.step(Event::Removed(first.file));
            model.check(&rig, 13);
            assert_eq!(routing_sizes(&rig.source), (2, 2, 4));
            for generation in 0..36 {
                let labels = if (generation + trace) % 2 == 0 {
                    [Label::A, Label::B]
                } else {
                    [Label::B, Label::A]
                };
                let next = rig.image(
                    &format!("output-{generation}"),
                    Registration::Target,
                    &labels,
                    generation + 1,
                );
                model.step(Event::Published(&next));
                model.step(Event::Relocated {
                    from: current.file,
                    to: next.file,
                });
                let work = rig.source.relocation_steps.load(Ordering::Relaxed);
                let bytes = rig.fs.bytes_read();
                assert!(rig.source.moved(current.file, mapping(&current, &next)));
                assert_eq!(
                    rig.source.relocation_steps.load(Ordering::Relaxed) - work,
                    4
                );
                assert_eq!(
                    rig.fs.bytes_read(),
                    bytes,
                    "relocation must not read unrelated/history bytes"
                );
                rig.fs.remove_file(&current.path).unwrap();
                model.step(Event::Removed(current.file));
                assert!(
                    rig.source.read(current.spans[0].1).is_none(),
                    "private targets expose no fold aliases"
                );
                current = next;
                model.check(&rig, 13);
                assert_eq!(
                    routing_sizes(&rig.source),
                    (2, 2, 4),
                    "output generations must retain no new value maps"
                );
            }
        }
    }

    #[test]
    fn rejected_relocations_preserve_every_source_state_and_alias() {
        let rig = Rig::new();
        let exposed = rig.image("exposed", Registration::Exposed, &[Label::A, Label::B], 0);
        let target = rig.image("target", Registration::Target, &[Label::A, Label::B], 7);
        let aliases = rig.image("aliases", Registration::Exposed, &[Label::A], 2);
        assert!(rig.source.moved(aliases.file, mapping(&aliases, &target)));
        let retired = rig.image("retired", Registration::Target, &[Label::A], 4);
        assert!(rig.source.moved(retired.file, mapping(&retired, &target)));
        let private = rig.image("private", Registration::Target, &[Label::A], 6);
        let states = [&exposed, &private, &aliases, &retired];
        for source in states {
            for invalid_file in [source.file, aliases.file, retired.file, u32::MAX] {
                let before = format!("{:?}", rig.source.locations.read().unwrap());
                let mut map = mapping(source, &target);
                for value in map.values_mut() {
                    value.destination.file = invalid_file;
                }
                assert!(!rig.source.moved(source.file, map));
                assert_eq!(
                    format!("{:?}", rig.source.locations.read().unwrap()),
                    before
                );
            }
            let mut map = mapping(source, &target);
            map.values_mut().next().unwrap().source_len += 1;
            let before = format!("{:?}", rig.source.locations.read().unwrap());
            assert!(!rig.source.moved(source.file, map));
            assert_eq!(
                format!("{:?}", rig.source.locations.read().unwrap()),
                before
            );
        }
        let mut bad_known_length = mapping(&exposed, &target);
        let a = bad_known_length
            .values_mut()
            .find(|value| value.id == Label::A.id())
            .unwrap();
        a.source_len += 1;
        a.destination.len += 1;
        let before = format!("{:?}", rig.source.locations.read().unwrap());
        assert!(!rig.source.moved(exposed.file, bad_known_length));
        assert!(!rig.source.moved(u32::MAX, mapping(&exposed, &target)));
        assert_eq!(
            format!("{:?}", rig.source.locations.read().unwrap()),
            before
        );
        let incompatible_new_identity = HashMap::from([
            (
                101,
                RelocatedValue {
                    id: Label::B.id(),
                    source_len: 1,
                    destination: ValueAt {
                        file: target.file,
                        offset: 0,
                        len: 1,
                    },
                },
            ),
            (
                102,
                RelocatedValue {
                    id: Label::B.id(),
                    source_len: 2,
                    destination: ValueAt {
                        file: target.file,
                        offset: 0,
                        len: 2,
                    },
                },
            ),
        ]);
        assert!(!rig.source.moved(exposed.file, incompatible_new_identity));
        assert_eq!(
            format!("{:?}", rig.source.locations.read().unwrap()),
            before
        );
        assert!(rig.source.read(private.spans[0].1).is_none());
        assert!(rig.source.read(retired.spans[0].1).is_none());
        let mut wrong_alias_length = aliases.spans[0].1;
        wrong_alias_length.len += 1;
        assert!(rig.source.read(wrong_alias_length).is_none());
        for image in [&exposed, &aliases] {
            for (label, at) in &image.spans {
                assert_eq!(rig.source.read(*at).as_deref(), Some(label.bytes()));
            }
        }
    }

    #[test]
    fn every_source_state_can_retarget_into_either_live_exposure() {
        for source_state in 0..4 {
            for target_exposure in [Registration::Exposed, Registration::Target] {
                let rig = Rig::new();
                let original =
                    rig.image("original", Registration::Exposed, &[Label::A, Label::B], 3);
                let private = rig.image("private", Registration::Target, &[Label::B, Label::A], 7);
                let next = rig.image("next", Registration::Target, &[Label::A, Label::B], 11);
                let from = match source_state {
                    0 => &original,
                    1 => {
                        assert!(
                            rig.source
                                .moved(original.file, mapping(&original, &private))
                        );
                        &private
                    }
                    2 => {
                        assert!(
                            rig.source
                                .moved(original.file, mapping(&original, &private))
                        );
                        &original
                    }
                    3 => {
                        assert!(
                            rig.source
                                .moved(original.file, mapping(&original, &private))
                        );
                        assert!(rig.source.moved(private.file, mapping(&private, &next)));
                        &private
                    }
                    _ => panic!("unexpected"),
                };
                let destination =
                    rig.image("destination", target_exposure, &[Label::B, Label::A], 29);
                assert!(rig.source.moved(from.file, mapping(from, &destination)));
                rig.fs.remove_file(&from.path).unwrap();
                for (label, at) in &original.spans {
                    assert_eq!(rig.source.read(*at).as_deref(), Some(label.bytes()));
                }
            }
        }
    }

    #[test]
    fn a_published_target_needs_no_open_when_its_relocated_values_are_read() {
        let rig = Rig::new();
        let slots = rig.source.locations.read().unwrap().slots.len();
        assert_eq!(
            rig.source
                .add_target(Path::new("/values/absent"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(rig.source.locations.read().unwrap().slots.len(), slots);

        let original = rig.image(
            "original",
            Registration::Exposed,
            &[Label::A, Label::B, Label::A],
            3,
        );
        let target = rig.image("target", Registration::Target, &[Label::B, Label::A], 11);
        assert_eq!(
            rig.fs
                .opened()
                .iter()
                .filter(|path| **path == target.path)
                .count(),
            1
        );
        assert!(rig.source.moved(original.file, mapping(&original, &target)));
        rig.fs.remove_file(&original.path).unwrap();
        rig.fs.clear_log();
        let lookups = rig.source.lookup_steps.load(Ordering::Relaxed);
        for _ in 0..64 {
            for (label, at) in &original.spans {
                assert_eq!(rig.source.read(*at).as_deref(), Some(label.bytes()));
            }
        }
        assert!(
            rig.fs.opened().is_empty(),
            "published target reads must use the retained handle"
        );
        assert_eq!(
            rig.source.lookup_steps.load(Ordering::Relaxed) - lookups,
            64 * 3 * 3
        );
        let one_read: u64 = original
            .spans
            .iter()
            .map(|(label, _)| label.bytes().len() as u64)
            .sum();
        assert_eq!(rig.fs.bytes_read(), 64 * one_read);
    }

    #[test]
    fn a_reader_acquires_its_handle_before_relocation_can_unlink_its_path() {
        use crate::vfs::{DirEntry, FileMeta, LockHandle, WriteHandle};
        use std::io;
        use std::sync::atomic::AtomicBool;
        use std::sync::mpsc::{self, Receiver, SyncSender};
        use std::time::Duration;

        struct PausedRead {
            inner: Box<dyn ReadHandle>,
            proceed: Arc<Mutex<Option<Receiver<()>>>>,
        }

        impl ReadHandle for PausedRead {
            fn len(&self) -> u64 {
                self.inner.len()
            }
            fn read_at(&self, bytes: &mut [u8], offset: u64) -> io::Result<()> {
                if let Some(proceed) = self.proceed.lock().unwrap().take() {
                    proceed.recv().unwrap();
                }
                self.inner.read_at(bytes, offset)
            }
        }

        struct PausedOpen {
            fs: FaultFs,
            path: PathBuf,
            first: AtomicBool,
            entered: SyncSender<()>,
            open: Mutex<Receiver<()>>,
            read: Arc<Mutex<Option<Receiver<()>>>>,
        }

        impl std::fmt::Debug for PausedOpen {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_tuple("PausedOpen").field(&self.path).finish()
            }
        }

        impl Vfs for PausedOpen {
            fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
                self.fs.read(path)
            }
            fn open(&self, path: &Path) -> io::Result<Box<dyn ReadHandle>> {
                if path == self.path && self.first.swap(false, Ordering::SeqCst) {
                    self.entered.send(()).unwrap();
                    self.open.lock().unwrap().recv().unwrap();
                    Ok(Box::new(PausedRead {
                        inner: self.fs.open(path)?,
                        proceed: self.read.clone(),
                    }))
                } else {
                    self.fs.open(path)
                }
            }
            fn metadata(&self, path: &Path) -> io::Result<FileMeta> {
                self.fs.metadata(path)
            }
            fn read_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>> {
                self.fs.read_dir(path)
            }
            fn create_new(&self, path: &Path) -> io::Result<Box<dyn WriteHandle>> {
                self.fs.create_new(path)
            }
            fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
                self.fs.rename(from, to)
            }
            fn rename_file_noreplace(&self, from: &Path, to: &Path) -> io::Result<()> {
                self.fs.rename_file_noreplace(from, to)
            }
            fn rename_dir_noreplace(&self, from: &Path, to: &Path) -> io::Result<()> {
                self.fs.rename_dir_noreplace(from, to)
            }
            fn remove_file(&self, path: &Path) -> io::Result<()> {
                self.fs.remove_file(path)
            }
            fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
                self.fs.remove_dir_all(path)
            }
            fn create_dir(&self, path: &Path) -> io::Result<()> {
                self.fs.create_dir(path)
            }
            fn sync_dir(&self, path: &Path) -> io::Result<()> {
                self.fs.sync_dir(path)
            }
            fn sync_file(&self, path: &Path) -> io::Result<()> {
                self.fs.sync_file(path)
            }
            fn lock(&self, path: &Path) -> io::Result<Option<Box<dyn LockHandle>>> {
                self.fs.lock(path)
            }
            fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
                self.fs.canonicalize(path)
            }
            fn outside_dir(&self, path: &Path) -> io::Result<PathBuf> {
                self.fs.outside_dir(path)
            }
        }

        let fs = FaultFs::new();
        fs.create_dir(Path::new("/values")).unwrap();
        let old_path = Path::new("/values/old");
        let new_path = Path::new("/values/new");
        fs.put_unseen(old_path, Label::A.bytes(), None);
        fs.put_unseen(new_path, Label::A.bytes(), None);
        let (entered_tx, entered) = mpsc::sync_channel(1);
        let (open_tx, open) = mpsc::channel();
        let (read_tx, read) = mpsc::channel();
        let wrapped = PausedOpen {
            fs: fs.clone(),
            path: old_path.to_owned(),
            first: AtomicBool::new(true),
            entered: entered_tx,
            open: Mutex::new(open),
            read: Arc::new(Mutex::new(Some(read))),
        };
        let source = Arc::new(FileSource::new(Arc::new(wrapped)));
        let old = source.add(old_path);
        let new = source.add_target(new_path).unwrap();
        let original = ValueAt {
            file: old,
            offset: 0,
            len: Label::A.bytes().len() as u32,
        };
        let reading = source.clone();
        let reader = std::thread::spawn(move || reading.read(original));
        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        let acquisition_is_protected = source.locations.try_write().is_err();
        let (done_tx, done) = mpsc::channel();
        let moving = source.clone();
        let removing = fs.clone();
        let mover = std::thread::spawn(move || {
            let moved = moving.moved(
                old,
                HashMap::from([(
                    0,
                    RelocatedValue {
                        id: Label::A.id(),
                        source_len: original.len,
                        destination: ValueAt {
                            file: new,
                            ..original
                        },
                    },
                )]),
            );
            let removed = moved && removing.remove_file(Path::new("/values/old")).is_ok();
            done_tx.send((moved, removed)).unwrap();
        });
        open_tx.send(()).unwrap();
        let moved_before_data_read = done.recv_timeout(Duration::from_secs(5));
        // Release before assertions so a failure cannot strand either test thread.
        read_tx.send(()).unwrap();
        let bytes = reader.join().unwrap();
        mover.join().unwrap();
        assert!(acquisition_is_protected);
        assert_eq!(moved_before_data_read.unwrap(), (true, true));
        assert_eq!(bytes.as_deref(), Some(Label::A.bytes()));
        assert_eq!(source.read(original).as_deref(), Some(Label::A.bytes()));
    }
}
