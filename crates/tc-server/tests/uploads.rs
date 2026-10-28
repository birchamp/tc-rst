//! Uploads (APP-DESIGN §5.5, AD26, A-28): streamed to app data with per-flow caps, whole or
//! not at all.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use std::io::{Read as _, Write as _};
use std::net::{Shutdown, TcpStream};
use std::time::{Duration, Instant};

use common::{Req, TestServer, read_response, stub};
use tc_api::ApiError;
use tc_api::Reply;
use tc_api::refusal::{Refusal, UploadRefusal};
use tc_api::upload::{UPLOAD_FLOW_HEADER, UPLOAD_NAME_HEADER, UploadFlow, UploadView};
use tc_server::{Server, ServerConfig};

const MIB: usize = 1024 * 1024;

fn bytes(len: usize) -> Vec<u8> {
    (0..len).map(|i| ((i * 31 + i / 251) % 251) as u8).collect()
}

fn files_in(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Waits for `dir` to hold no file; a cut upload is cleaned up by the dropped handler, a
/// moment after the socket closes.
fn assert_empties(dir: &std::path::Path) {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let left = files_in(dir);
        if left.is_empty() {
            return;
        }
        assert!(Instant::now() < until, "left behind: {left:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn view(response: &common::Response) -> UploadView {
    assert_eq!(response.status, 200, "{}", response.text());
    match serde_json::from_slice::<Reply<UploadView>>(&response.body).unwrap() {
        Reply::Ok(view) => view,
        Reply::Err(error) => panic!("refused: {error:?}"),
    }
}

fn error(response: &common::Response) -> ApiError {
    assert_eq!(response.status, 200, "{}", response.text());
    match serde_json::from_slice::<Reply<UploadView>>(&response.body).unwrap() {
        Reply::Ok(view) => panic!("accepted: {view:?}"),
        Reply::Err(error) => error,
    }
}

fn bad_field(response: &common::Response) -> String {
    match error(response) {
        ApiError::BadRequest { field, .. } => field,
        other => panic!("not a bad request: {other:?}"),
    }
}

/// Sends the head with a declared length, as the browser does before the body.
fn open_upload(
    server: &TestServer,
    flow: UploadFlow,
    declared: u64,
    read_timeout: Duration,
) -> TcpStream {
    let request = server
        .upload_req(flow, "Big.zip", b"")
        .with("Content-Length", &declared.to_string());
    let mut stream = server.connect();
    stream.set_read_timeout(Some(read_timeout)).unwrap();
    stream.write_all(&request.head_bytes()).unwrap();
    stream
}

#[test]
fn an_upload_streams_into_uploads_and_answers_its_view() {
    let server = TestServer::start(stub());
    let body = bytes(3 * MIB);
    let name = "Tito \u{2013} 1.usfm";
    let response = server.send(&server.upload_req(UploadFlow::Usfm, name, &body));
    let view = view(&response);
    assert_eq!(view.name, name);
    assert_eq!(view.size, body.len() as u64);
    let dir = server.uploads_dir();
    assert_eq!(files_in(&dir), [view.id.as_str()], "no .part is left");
    assert_eq!(std::fs::read(dir.join(view.id.as_str())).unwrap(), body);
    let upload = server.handle.uploads().get(&view.id).expect("registered");
    assert_eq!(upload.name, name);
    assert_eq!(upload.size, body.len() as u64);
    assert_eq!(upload.flow, UploadFlow::Usfm);
    assert_eq!(upload.path, dir.join(view.id.as_str()));
    assert!(server.handle.uploads().take(&view.id).is_some());
    assert!(server.handle.uploads().take(&view.id).is_none());
    assert!(server.handle.uploads().get(&view.id).is_none());
}

#[test]
fn a_513_mib_import_upload_is_413() {
    let server = TestServer::start(stub());
    let declared = 513 * MIB as u64;
    let mut stream = open_upload(
        &server,
        UploadFlow::Import,
        declared,
        Duration::from_secs(2),
    );
    // The body is never sent: the cap is checked on the declared length.
    let began = Instant::now();
    let response = read_response(&mut stream);
    assert_eq!(response.status, 413, "{}", response.text());
    assert_eq!(response.header("connection"), Some("close"));
    assert!(began.elapsed() < Duration::from_secs(2));
    assert_eq!(files_in(&server.uploads_dir()), Vec::<String>::new());
}

#[test]
fn an_import_upload_of_exactly_the_cap_is_taken_up() {
    // The cap is 512 MiB: one byte more is refused, the size itself is not. Shown with a
    // small cap so no test writes half a gigabyte.
    let server = TestServer::start_with(stub(), |c| c.limits.upload_cap = |_| MIB as u64, true);
    let ok = server.send(&server.upload_req(UploadFlow::Import, "a", &bytes(MIB)));
    assert_eq!(view(&ok).size, MIB as u64);
    let over = server.send(&server.upload_req(UploadFlow::Import, "a", &bytes(MIB + 1)));
    assert_eq!(over.status, 413);
}

#[test]
fn an_upload_well_over_its_cap_is_taken_whole_and_answered_413() {
    let server = TestServer::start_with(stub(), |c| c.limits.upload_cap = |_| MIB as u64, true);
    // More than loopback buffers hold: a server that closed on the unread rest would cut the
    // client off while it writes, as it would a browser.
    let request = server.upload_req(UploadFlow::Import, "a", &vec![0; 32 * MIB]);
    let mut stream = server.connect();
    stream
        .write_all(&request.to_bytes())
        .expect("the server takes the whole body");
    let response = read_response(&mut stream);
    assert_eq!(response.status, 413, "{}", response.text());
    assert_empties(&server.uploads_dir());
}

#[test]
fn an_exchange_upload_may_be_larger_than_512_mib() {
    let server = TestServer::start(stub());
    let declared = 513 * MIB as u64;
    let mut stream = open_upload(
        &server,
        UploadFlow::Exchange,
        declared,
        Duration::from_millis(300),
    );
    stream.write_all(&bytes(MIB)).unwrap();
    // The server is taking the body, not refusing it: nothing comes back.
    let mut buf = [0_u8; 64];
    let read = stream.read(&mut buf);
    assert!(
        matches!(&read, Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut)),
        "the server answered a large exchange upload early: {read:?}"
    );
    let dir = server.uploads_dir();
    let names = files_in(&dir);
    assert_eq!(names.len(), 1, "{names:?}");
    assert!(names[0].ends_with(".part"), "{names:?}");
    drop(stream);
    assert_empties(&dir);
}

/// A chunked request body: the head, then each piece as one chunk, then the end.
fn chunked(server: &TestServer, flow: UploadFlow, pieces: &[&[u8]]) -> common::Response {
    let request = server
        .upload_req(flow, "chunks.bin", b"")
        .with("Transfer-Encoding", "chunked");
    let mut stream = server.connect();
    stream.write_all(&request.head_bytes()).unwrap();
    for piece in pieces {
        // The server may answer and close before the last piece; that is what is tested.
        let _ = stream.write_all(format!("{:x}\r\n", piece.len()).as_bytes());
        let _ = stream.write_all(piece);
        let _ = stream.write_all(b"\r\n");
    }
    let _ = stream.write_all(b"0\r\n\r\n");
    read_response(&mut stream)
}

#[test]
fn an_upload_past_its_cap_without_a_length_is_413_and_leaves_nothing() {
    let server = TestServer::start_with(stub(), |c| c.limits.upload_cap = |_| MIB as u64, true);
    let body = bytes(MIB + 1);
    let response = chunked(&server, UploadFlow::Usfm, &[&body[..MIB], &body[MIB..]]);
    assert_eq!(response.status, 413, "{}", response.text());
    assert_empties(&server.uploads_dir());
    let exact = chunked(&server, UploadFlow::Usfm, &[&body[..MIB]]);
    assert_eq!(view(&exact).size, MIB as u64);
}

#[test]
fn an_upload_stores_exactly_its_declared_length() {
    let server = TestServer::start(stub());
    let body = bytes(MIB + 10);
    let request = server
        .upload_req(UploadFlow::Usfm, "x.usfm", b"")
        .with("Content-Length", &MIB.to_string());
    let mut stream = server.connect();
    stream.write_all(&request.head_bytes()).unwrap();
    // The ten bytes past the declared length are not part of the upload.
    let _ = stream.write_all(&body);
    let response = read_response(&mut stream);
    let view = view(&response);
    assert_eq!(view.size, MIB as u64);
    let stored = std::fs::read(server.uploads_dir().join(view.id.as_str())).unwrap();
    assert_eq!(stored, body[..MIB]);
}

#[test]
fn an_upload_cut_off_leaves_nothing() {
    let server = TestServer::start(stub());
    let mut stream = open_upload(
        &server,
        UploadFlow::Usfm,
        MIB as u64,
        Duration::from_secs(2),
    );
    stream.write_all(&bytes(MIB / 2)).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    stream.shutdown(Shutdown::Both).unwrap();
    drop(stream);
    assert_empties(&server.uploads_dir());
}

#[test]
fn an_upload_needs_a_name_and_a_flow() {
    let server = TestServer::start(stub());
    let body = bytes(100);
    let base = || server.upload_req(UploadFlow::Usfm, "a.usfm", &body);
    let name_cases: [(&str, Req); 8] = [
        ("no name", base().without(UPLOAD_NAME_HEADER)),
        (
            "bad percent-encoding",
            base().with(UPLOAD_NAME_HEADER, "%zz"),
        ),
        ("a stray percent", base().with(UPLOAD_NAME_HEADER, "a%")),
        ("empty", base().with(UPLOAD_NAME_HEADER, "")),
        (
            "a control character",
            base().with(UPLOAD_NAME_HEADER, "a%07b"),
        ),
        ("not UTF-8", base().with(UPLOAD_NAME_HEADER, "%ff%fe")),
        (
            "256 bytes",
            base().with(UPLOAD_NAME_HEADER, &"a".repeat(256)),
        ),
        ("twice", base().add(UPLOAD_NAME_HEADER, "b")),
    ];
    for (what, request) in name_cases {
        let response = server.send(&request);
        assert_eq!(bad_field(&response), UPLOAD_NAME_HEADER, "{what}");
        assert_eq!(response.header("connection"), Some("close"), "{what}");
    }
    let flow_cases: [(&str, Req); 3] = [
        ("no flow", base().without(UPLOAD_FLOW_HEADER)),
        ("an unknown flow", base().with(UPLOAD_FLOW_HEADER, "zip")),
        (
            "a flow in the wrong case",
            base().with(UPLOAD_FLOW_HEADER, "Usfm"),
        ),
    ];
    for (what, request) in flow_cases {
        let response = server.send(&request);
        assert_eq!(bad_field(&response), UPLOAD_FLOW_HEADER, "{what}");
    }
    // 255 bytes is the longest name; the server keeps it whole.
    let long = "a".repeat(255);
    let kept = server.send(&server.upload_req(UploadFlow::GuidedFix, &long, &body));
    assert_eq!(view(&kept).name, long);
    assert_eq!(
        files_in(&server.uploads_dir()).len(),
        1,
        "only the good one"
    );
}

#[test]
fn a_name_is_a_label_and_never_a_path() {
    let server = TestServer::start(stub());
    let name = "../../secret/..\\x:y.usfm";
    let response = server.send(&server.upload_req(UploadFlow::Usfm, name, b"abc"));
    let view = view(&response);
    assert_eq!(view.name, name);
    assert_eq!(files_in(&server.uploads_dir()), [view.id.as_str()]);
    assert_eq!(
        files_in(server.app_data.path()),
        ["downloads", "uploads"],
        "nothing was written outside uploads/"
    );
}

#[test]
fn an_upload_that_cannot_be_written_is_refused_and_leaves_nothing() {
    let server = TestServer::start(stub());
    // A file where the folder was: no `.part` can be created in it, as a read-only or a
    // vanished folder would not allow either (a file works for root and on Windows too).
    std::fs::remove_dir_all(server.uploads_dir()).unwrap();
    std::fs::write(server.uploads_dir(), b"in the way").unwrap();
    let response = server.send(&server.upload_req(UploadFlow::Usfm, "a.usfm", b"abc"));
    match error(&response) {
        ApiError::Refused(Refusal::Upload(UploadRefusal::NotWritten { reason })) => {
            assert!(!reason.is_empty());
        }
        other => panic!("not NotWritten: {other:?}"),
    }
    assert_eq!(response.header("connection"), Some("close"));
}

#[test]
fn uploads_and_downloads_are_emptied_at_start() {
    let app_data = tempfile::tempdir().unwrap();
    let ui = tempfile::tempdir().unwrap();
    for file in [
        "uploads/old-upload",
        "uploads/old.part",
        "downloads/old-download",
        "downloads/nested/deeper/file",
        "keep.txt",
        "prefs/app.json",
    ] {
        let path = app_data.path().join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }
    let _server = Server::bind(ServerConfig::new(
        app_data.path().to_path_buf(),
        ui.path().to_path_buf(),
    ))
    .unwrap();
    assert_eq!(
        files_in(&app_data.path().join("uploads")),
        Vec::<String>::new()
    );
    assert_eq!(
        files_in(&app_data.path().join("downloads")),
        Vec::<String>::new()
    );
    assert!(app_data.path().join("keep.txt").exists());
    assert!(app_data.path().join("prefs/app.json").exists());
}

#[test]
fn an_upload_needs_the_api_headers() {
    let server = TestServer::start(stub());
    let good = server.upload_req(UploadFlow::Usfm, "a.usfm", b"abc");
    let no_version = server.send(&good.clone().without(tc_api::endpoint::API_VERSION_HEADER));
    assert_eq!(no_version.status, 409);
    let no_origin = server.send(&good.clone().without("Origin"));
    assert_eq!(no_origin.status, 403);
    let no_token = server.send(&good.clone().without(tc_api::endpoint::TOKEN_HEADER));
    assert_eq!(no_token.status, 401);
    assert_eq!(files_in(&server.uploads_dir()), Vec::<String>::new());
    assert_eq!(view(&server.send(&good)).size, 3);
}

#[test]
fn an_upload_is_a_post_only() {
    let server = TestServer::start(stub());
    let request = server
        .authed("GET", tc_api::upload::UPLOADS_PATH)
        .with(tc_api::endpoint::API_VERSION_HEADER, "1");
    assert_eq!(server.send(&request).status, 405);
}

#[test]
fn an_upload_no_job_takes_is_forgotten_after_a_day() {
    let server = TestServer::start(stub());
    let response = server.send(&server.upload_req(UploadFlow::Exchange, "a.tcz", b"a"));
    let stored = Instant::now();
    let id = view(&response).id;
    let uploads = server.handle.uploads();
    assert!(
        uploads
            .sweep(stored + Duration::from_secs(59 * 60))
            .is_empty(),
        "kept for 59 min"
    );
    assert!(uploads.get(&id).is_some());
    let removed = uploads.sweep(Instant::now() + Duration::from_secs(24 * 60 * 60));
    assert_eq!(removed, [server.uploads_dir().join(id.as_str())]);
    assert!(uploads.get(&id).is_none());
    assert!(
        uploads.take(&id).is_none(),
        "a swept upload cannot be taken"
    );
}

#[test]
fn the_sweeper_deletes_an_abandoned_upload_and_never_a_taken_one() {
    let server = TestServer::start_with(
        stub(),
        |config| {
            config.limits.uploads_keep = Duration::from_millis(100);
            config.limits.downloads_sweep = Duration::from_millis(50);
        },
        true,
    );
    let abandoned = view(&server.send(&server.upload_req(UploadFlow::Usfm, "a.usfm", b"a"))).id;
    let read = view(&server.send(&server.upload_req(UploadFlow::Exchange, "b.tcz", b"b"))).id;
    let taken = server.handle.uploads().take(&read).expect("stored");
    let gone = server.uploads_dir().join(abandoned.as_str());
    let until = Instant::now() + Duration::from_secs(5);
    while gone.exists() {
        assert!(
            Instant::now() < until,
            "the abandoned upload was never deleted"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    // Several sweeps past the keep time: the job that took the file still reads it.
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(std::fs::read(&taken.path).unwrap(), b"b");
}

#[test]
fn a_dropped_upload_is_deleted_and_a_taken_one_is_left_to_its_job() {
    let server = TestServer::start(stub());
    let uploads = server.handle.uploads();
    let picked = view(&server.send(&server.upload_req(UploadFlow::Usfm, "a.usfm", b"a"))).id;
    let file = server.uploads_dir().join(picked.as_str());
    assert!(uploads.drop_stored(&picked));
    assert!(!file.exists(), "a dropped upload is deleted at once");
    assert!(!uploads.drop_stored(&picked), "a second drop finds nothing");

    let read = view(&server.send(&server.upload_req(UploadFlow::Exchange, "b.tcz", b"b"))).id;
    let taken = uploads.take(&read).expect("stored");
    assert!(!uploads.drop_stored(&read), "a taken upload is not dropped");
    assert_eq!(
        std::fs::read(&taken.path).unwrap(),
        b"b",
        "the job's file stays"
    );
}
