//! Downloads (APP-DESIGN §5.5): a finished file is registered, fetched as an attachment for
//! ten minutes, and then forgotten.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use std::time::{Duration, Instant};

use common::{TestServer, stub};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use tc_server::DownloadError;

const HTML: &str = "text/html; charset=utf-8";

fn url(id: &str, name: &str) -> String {
    format!(
        "/download/{id}/{}",
        utf8_percent_encode(name, NON_ALPHANUMERIC)
    )
}

/// Writes `bytes` into the downloads folder and registers them.
fn add(server: &TestServer, name: &str, bytes: &[u8]) -> String {
    let written = server.handle.downloads().dir().join("work.tmp");
    std::fs::write(&written, bytes).unwrap();
    let id = server
        .handle
        .downloads()
        .add(&written, name, HTML)
        .unwrap()
        .as_str()
        .to_owned();
    assert!(!written.exists(), "the producer's file moved to its id");
    id
}

#[test]
fn a_download_is_served_as_an_attachment() {
    let server = TestServer::start(stub());
    let bytes = b"<p>Titus 1:1</p>".repeat(100);
    let name = "Titus \u{2013} PDF.html";
    let id = add(&server, name, &bytes);
    for _ in 0..2 {
        // More than one fetch within the time works.
        let response = server.get(&url(&id, name));
        assert_eq!(response.status, 200, "{}", response.text());
        assert_eq!(response.body, bytes);
        assert_eq!(response.header("content-type"), Some(HTML));
        assert_eq!(
            response.header("content-length"),
            Some(bytes.len().to_string().as_str())
        );
        assert_eq!(
            response.header("content-disposition"),
            Some(
                "attachment; filename=\"Titus _ PDF.html\"; \
                 filename*=UTF-8''Titus%20%E2%80%93%20PDF%2Ehtml"
            )
        );
        assert_eq!(response.header("cache-control"), Some("no-store"));
    }
}

#[test]
fn a_filename_cannot_end_its_quoted_string() {
    let server = TestServer::start(stub());
    let name = "a\"b%c;d.html";
    let id = add(&server, name, b"x");
    let response = server.get(&url(&id, name));
    assert_eq!(response.status, 200);
    let disposition = response.header("content-disposition").unwrap();
    assert!(
        disposition.starts_with("attachment; filename=\"a_b_c_d.html\"; filename*=UTF-8''"),
        "{disposition}"
    );
}

#[test]
fn a_large_download_arrives_whole() {
    let server = TestServer::start(stub());
    let bytes: Vec<u8> = (0..3_000_000_u32).map(|i| (i % 253) as u8).collect();
    let id = add(&server, "big.html", &bytes);
    let response = server.get(&url(&id, "big.html"));
    assert_eq!(response.status, 200);
    assert_eq!(response.body.len(), bytes.len());
    assert!(response.body == bytes, "the bytes differ");
}

#[test]
fn a_download_with_another_name_or_an_unknown_id_is_404() {
    let server = TestServer::start(stub());
    let id = add(&server, "one.html", b"1");
    assert_eq!(
        server.get(&url(&id, "two.html")).status,
        404,
        "another name"
    );
    assert_eq!(
        server.get(&url(&id, "One.html")).status,
        404,
        "case matters"
    );
    let unknown = "0".repeat(32);
    assert_eq!(
        server.get(&url(&unknown, "one.html")).status,
        404,
        "unknown id"
    );
    assert_eq!(
        server.get(&url("abc", "one.html")).status,
        404,
        "not 32 hex"
    );
    assert_eq!(
        server.get(&url(&id.to_uppercase(), "one.html")).status,
        404,
        "uppercase hex"
    );
    assert_eq!(
        server.get(&format!("/download/{id}")).status,
        404,
        "no name"
    );
    assert_eq!(
        server.get(&format!("/download/{id}/one.html/x")).status,
        404
    );
    assert_eq!(
        server.get(&url(&id, "one.html")).status,
        200,
        "the right one"
    );
}

#[test]
fn a_download_needs_the_token() {
    let server = TestServer::start(stub());
    let id = add(&server, "one.html", b"1");
    let bare = common::Req::new("GET", &url(&id, "one.html"))
        .with("Host", &format!("127.0.0.1:{}", server.port));
    assert_eq!(server.send(&bare).status, 401);
}

#[test]
fn a_download_name_with_a_separator_or_a_bad_character_is_refused() {
    let server = TestServer::start(stub());
    let written = server.handle.downloads().dir().join("work.tmp");
    let long = "a".repeat(256);
    for name in ["a/b", "a\\b", "", "tab\there", long.as_str()] {
        std::fs::write(&written, b"x").unwrap();
        let result = server.handle.downloads().add(&written, name, HTML);
        assert!(
            matches!(result, Err(DownloadError::BadName(_))),
            "{name:?}: {result:?}"
        );
        assert!(written.exists(), "a refused file stays where it was");
    }
}

#[test]
fn downloads_expire_after_10_minutes() {
    let server = TestServer::start(stub());
    let id = add(&server, "one.html", b"1");
    let kept_at = server.handle.downloads().dir().join(&id);
    let after_add = Instant::now();
    let downloads = server.handle.downloads();
    assert!(
        downloads
            .sweep(after_add + Duration::from_secs(9 * 60 + 59))
            .is_empty(),
        "kept for 9 min 59 s"
    );
    assert_eq!(server.get(&url(&id, "one.html")).status, 200);
    let removed = downloads.sweep(after_add + Duration::from_secs(10 * 60 + 1));
    assert_eq!(removed, [kept_at], "forgotten after 10 min 1 s");
    assert_eq!(server.get(&url(&id, "one.html")).status, 404);
}

#[test]
fn the_sweeper_deletes_expired_files() {
    let server = TestServer::start_with(
        stub(),
        |config| {
            config.limits.downloads_keep = Duration::from_millis(100);
            config.limits.downloads_sweep = Duration::from_millis(50);
        },
        true,
    );
    let id = add(&server, "one.html", b"1");
    let file = server.handle.downloads().dir().join(&id);
    assert!(file.exists());
    let until = Instant::now() + Duration::from_secs(5);
    while file.exists() {
        assert!(Instant::now() < until, "the file was never deleted");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(server.get(&url(&id, "one.html")).status, 404);
}
