//! Scale corpora (`docs/PERFORMANCE.md`) written to disk as projects through the real open,
//! seal, consolidation and materialization: the corpus's packs and pending files go into a
//! project folder whose own device is the corpus's device 0, and a checkpoint seals and
//! materializes them.

use crate::common::*;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tc_corpus::{Corpus, Spec};
use tc_journal::format::FormatDoc;
use tc_store::fault::FaultFs;
use tc_store::layout::{Folder, GITATTRIBUTES_BYTES, GITIGNORE_BYTES};
use tc_store::{OpenOptions, Store};

/// Writes a corpus's files into `folder` on `fs`, with `project.json`, `format.json` and
/// the folder's device, as an import would leave them before its first open.
pub fn put_corpus(store: &Store, fs: &FaultFs, folder: &Path, corpus: &Corpus) {
    let journal = &corpus.journal;
    let f = Folder::new(folder);
    let project = serde_json::json!({
        "schemaVersion": 1,
        "projectId": journal.project.to_string(),
        "kind": "bible",
        "created": "2026-10-17T00:00:00.000Z",
    });
    fs.put(
        &f.path("ingredients/tc/project.json"),
        project.to_string().as_bytes(),
        None,
    );
    fs.put(
        &f.path("ingredients/tc/format.json"),
        &FormatDoc::current().to_bytes(),
        None,
    );
    fs.put(&f.path(".gitignore"), GITIGNORE_BYTES, None);
    fs.put(&f.path(".gitattributes"), GITATTRIBUTES_BYTES, None);
    for pack in &journal.packs {
        fs.put(&f.path(&pack.path), &pack.bytes, None);
    }
    for pending in &journal.pending {
        fs.put(
            &f.path(&format!(".tc/pending/{}", pending.name)),
            &pending.bytes,
            None,
        );
    }
    // The corpus's device 0 is this folder's device (§6.3).
    let print = tc_store::device::fingerprint(
        store.app_data().secret(),
        "machine-of-host-1",
        &folder.to_string_lossy(),
    );
    let device = serde_json::json!({
        "device": journal.devices[0].to_string(),
        "fingerprint": print.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        "created": "2026-10-17T00:00:00.000Z",
    });
    fs.put(
        &f.path(".tc/device.json"),
        device.to_string().as_bytes(),
        None,
    );
}

/// A corpus project on a `FaultFs`: opened once (which materializes every file), sealed
/// and consolidated by a checkpoint, and closed. Each test takes a copy.
pub struct OnDisk {
    pub fs: FaultFs,
    pub folder: PathBuf,
}

fn build(spec: &Spec) -> OnDisk {
    let corpus = Corpus::generate(spec).unwrap();
    let fs = FaultFs::new();
    let rig = rig(fs.clone());
    let folder = corpus_folder();
    put_corpus(&rig.store, &fs, &folder, &corpus);
    let handle = rig.store.open(&folder, &OpenOptions::default()).unwrap();
    handle
        .checkpoint(tc_store::CheckpointReason::AfterImport)
        .wait()
        .unwrap();
    handle.wait_idle();
    handle.close().unwrap();
    assert_valid_burrito(&fs, &folder);
    OnDisk {
        fs: fs.crash(false),
        folder,
    }
}

fn corpus_folder() -> PathBuf {
    PathBuf::from(PROJECTS).join("corpus")
}

pub fn psalms() -> &'static OnDisk {
    static ONE: OnceLock<OnDisk> = OnceLock::new();
    ONE.get_or_init(|| cached("psalms", &Spec::psalms(1)))
}

pub fn psalms_larger() -> &'static OnDisk {
    static ONE: OnceLock<OnDisk> = OnceLock::new();
    let factor = Spec::test_factor();
    ONE.get_or_init(|| cached(&format!("psalms-x{factor}"), &Spec::scaled(1, factor)))
}

// ---------------------------------------------------------------------------------------
// The built corpora, kept between test processes: nextest runs each test in its own process,
// where the `OnceLock`s above start empty, and a build costs seconds.
// ---------------------------------------------------------------------------------------

/// Everything of a `FaultFs` a later open can see: its clock, its folders and files with their
/// modification times, and the real files of its git mirror.
#[derive(Debug, PartialEq, Eq)]
struct Image {
    now_ns: i64,
    dirs: Vec<(String, i64)>,
    files: Vec<(String, i64, Vec<u8>)>,
    git_dirs: Vec<String>,
    git_files: Vec<(String, Vec<u8>)>,
}

/// `name`'s corpus from the image the build left in the target folder, else built and left
/// there. The image's name holds a hash of the test binary and of the corpus's fixtures, so a
/// change to any code or data the build runs makes a new one; a lock lets one process build
/// while the others wait, and the rename makes the image appear whole or not at all.
#[expect(
    clippy::disallowed_methods,
    reason = "the test cache of built corpora in the target folder, which is no project's"
)]
fn cached(name: &str, spec: &Spec) -> OnDisk {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("tc-store-corpus");
    std::fs::create_dir_all(&dir).unwrap();
    let image = dir.join(format!("{name}-{}.img", image_key(name)));
    let lock = std::fs::File::create(dir.join(format!("{name}.lock"))).unwrap();
    lock.lock().unwrap();
    if let Ok(bytes) = std::fs::read(&image) {
        return OnDisk {
            fs: restore(&decode(&bytes)),
            folder: corpus_folder(),
        };
    }
    let built = build(spec);
    let taken = take(&built.fs);
    let bytes = encode(&taken);
    // The cache is only as good as the round trip: a corpus it gives must be the one built.
    let restored = restore(&decode(&bytes));
    assert!(
        take(&restored) == taken,
        "a corpus image does not give back the corpus"
    );
    assert!(
        take(&restored.crash(true)) == take(&built.fs.crash(true)),
        "a corpus image does not give back what a power cut keeps"
    );
    let partial = dir.join(format!("{name}.{}.partial", std::process::id()));
    std::fs::write(&partial, &bytes).unwrap();
    std::fs::rename(&partial, &image).unwrap();
    for entry in std::fs::read_dir(&dir).unwrap().flatten() {
        let file = entry.file_name().to_string_lossy().into_owned();
        if file.starts_with(&format!("{name}-")) && entry.path() != image {
            let _ = std::fs::remove_file(entry.path());
        }
    }
    built
}

/// The test binary (its path, size and modification time: cargo writes a new file for any
/// change of code) and every file of `fixtures/scale/psa/`, which the corpus is made from.
fn image_key(name: &str) -> String {
    let mut key = name.as_bytes().to_vec();
    let exe = std::env::current_exe().unwrap();
    let meta = std::fs::metadata(&exe).unwrap();
    let modified = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap();
    key.extend(format!("{}|{}|{modified:?}", exe.display(), meta.len()).as_bytes());
    let data = tc_corpus::repo_root().join("fixtures/scale/psa");
    let mut stack = vec![data.clone()];
    let mut files = Vec::new();
    while let Some(at) = stack.pop() {
        for entry in std::fs::read_dir(&at).unwrap().flatten() {
            if entry.file_type().unwrap().is_dir() {
                stack.push(entry.path());
            } else {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    for file in files {
        key.extend(slashed(file.strip_prefix(&data).unwrap()).as_bytes());
        key.extend(std::fs::read(&file).unwrap());
    }
    tc_journal::PackId::of(&key).to_hex()
}

fn take(fs: &FaultFs) -> Image {
    use tc_store::Vfs;
    use tc_store::vfs::EntryKind;
    let mut image = Image {
        now_ns: fs.now_ns(),
        dirs: Vec::new(),
        files: Vec::new(),
        git_dirs: Vec::new(),
        git_files: Vec::new(),
    };
    let mut stack = vec![PathBuf::from("/")];
    while let Some(at) = stack.pop() {
        for entry in fs.read_dir(&at).unwrap() {
            let path = at.join(&entry.name);
            let mtime = fs.metadata(&path).unwrap().mtime_ns;
            match entry.kind {
                EntryKind::Dir => {
                    image.dirs.push((slashed(&path), mtime));
                    stack.push(path);
                }
                _ => image
                    .files
                    .push((slashed(&path), mtime, fs.peek(&path).unwrap())),
            }
        }
    }
    let git = fs.outside_dir(Path::new("/")).unwrap();
    let mut stack = vec![git.clone()];
    while let Some(at) = stack.pop() {
        for entry in std::fs::read_dir(&at).unwrap().flatten() {
            let path = entry.path();
            let rel = slashed(path.strip_prefix(&git).unwrap());
            if entry.file_type().unwrap().is_dir() {
                image.git_dirs.push(rel);
                stack.push(path);
            } else {
                image.git_files.push((rel, std::fs::read(&path).unwrap()));
            }
        }
    }
    image.dirs.sort();
    image.files.sort();
    image.git_dirs.sort();
    image.git_files.sort();
    image
}

#[expect(
    clippy::disallowed_methods,
    reason = "the real files of the git mirror of a FaultFs made for tests"
)]
fn restore(image: &Image) -> FaultFs {
    use tc_store::Vfs;
    let fs = FaultFs::new();
    // `put` makes its folders durable at once, as the build's synced folders are.
    for (path, mtime, bytes) in &image.files {
        fs.put(Path::new(path), bytes, Some(*mtime));
    }
    for (path, _) in &image.dirs {
        let path = Path::new(path);
        if fs.metadata(path).is_err() {
            let placeholder = path.join("placeholder");
            fs.put(&placeholder, b"", None);
            fs.delete(&placeholder).unwrap();
        }
    }
    for (path, mtime) in &image.dirs {
        fs.set_mtime(Path::new(path), *mtime).unwrap();
    }
    let git = fs.outside_dir(Path::new("/")).unwrap();
    for dir in &image.git_dirs {
        std::fs::create_dir_all(git.join(dir)).unwrap();
    }
    for (path, bytes) in &image.git_files {
        std::fs::write(git.join(path), bytes).unwrap();
    }
    let behind = image.now_ns - fs.now_ns();
    fs.advance(std::time::Duration::from_nanos(
        u64::try_from(behind).expect("the restore's own steps are fewer than the build's"),
    ));
    fs.clear_log();
    fs
}

fn encode(image: &Image) -> Vec<u8> {
    fn put(out: &mut Vec<u8>, bytes: &[u8]) {
        out.extend((bytes.len() as u64).to_le_bytes());
        out.extend(bytes);
    }
    let mut out = b"tc-store corpus image 1\n".to_vec();
    out.extend(image.now_ns.to_le_bytes());
    out.extend((image.dirs.len() as u64).to_le_bytes());
    for (path, mtime) in &image.dirs {
        put(&mut out, path.as_bytes());
        out.extend(mtime.to_le_bytes());
    }
    out.extend((image.files.len() as u64).to_le_bytes());
    for (path, mtime, bytes) in &image.files {
        put(&mut out, path.as_bytes());
        out.extend(mtime.to_le_bytes());
        put(&mut out, bytes);
    }
    out.extend((image.git_dirs.len() as u64).to_le_bytes());
    for path in &image.git_dirs {
        put(&mut out, path.as_bytes());
    }
    out.extend((image.git_files.len() as u64).to_le_bytes());
    for (path, bytes) in &image.git_files {
        put(&mut out, path.as_bytes());
        put(&mut out, bytes);
    }
    out
}

fn decode(bytes: &[u8]) -> Image {
    struct Reader<'a>(&'a [u8]);
    impl Reader<'_> {
        fn take(&mut self, n: usize) -> &[u8] {
            let (head, rest) = self.0.split_at(n);
            self.0 = rest;
            head
        }
        fn u64(&mut self) -> u64 {
            u64::from_le_bytes(self.take(8).try_into().unwrap())
        }
        fn i64(&mut self) -> i64 {
            i64::from_le_bytes(self.take(8).try_into().unwrap())
        }
        fn bytes(&mut self) -> Vec<u8> {
            let n = usize::try_from(self.u64()).unwrap();
            self.take(n).to_vec()
        }
        fn text(&mut self) -> String {
            String::from_utf8(self.bytes()).unwrap()
        }
    }
    let magic = b"tc-store corpus image 1\n";
    assert!(bytes.starts_with(magic), "not a corpus image");
    let mut r = Reader(&bytes[magic.len()..]);
    let now_ns = r.i64();
    let dirs = (0..r.u64()).map(|_| (r.text(), r.i64())).collect();
    let files = (0..r.u64())
        .map(|_| (r.text(), r.i64(), r.bytes()))
        .collect();
    let git_dirs = (0..r.u64()).map(|_| r.text()).collect();
    let git_files = (0..r.u64()).map(|_| (r.text(), r.bytes())).collect();
    assert!(r.0.is_empty(), "a corpus image with bytes after its end");
    Image {
        now_ns,
        dirs,
        files,
        git_dirs,
        git_files,
    }
}
