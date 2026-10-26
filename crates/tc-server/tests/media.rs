//! `GET /media/obs/{pack}/{path}` (APP-DESIGN §5.3): a picture the backend serves, with the
//! token every route needs, and a 404 for what it does not.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use std::sync::Arc;

use common::{Req, TestServer};
use tc_server::{Answer, Backend, MediaFile};

/// Serves one picture and nothing else; every other call is the stub's.
struct Pictures;

impl Backend for Pictures {
    type Events = tc_server::stub::StubEvents;

    async fn call(&self, _path: &'static str, _body: tc_server::Bytes) -> Answer {
        Answer::NotFound
    }
    fn subscribe(&self, _: u64) -> Self::Events {
        tc_server::stub::StubEvents
    }
    async fn quit(&self) {}
    async fn quitting(&self) {
        std::future::pending::<()>().await;
    }
    async fn obs_picture(&self, pack: &str, path: &str) -> Option<MediaFile> {
        (pack == "git.door43.org~uW~obs_images_360~7146" && path == "360px/obs-en-01-01.jpg").then(
            || MediaFile {
                bytes: vec![0xFF, 0xD8, 0xFF, 0xD9],
                content_type: "image/jpeg",
            },
        )
    }
}

#[test]
fn a_picture_the_backend_serves_arrives_whole_with_its_type() {
    let server = TestServer::start(Arc::new(Pictures));
    let response =
        server.get("/media/obs/git.door43.org~uW~obs_images_360~7146/360px/obs-en-01-01.jpg");
    assert_eq!(response.status, 200);
    assert_eq!(response.body, [0xFF, 0xD8, 0xFF, 0xD9]);
    assert_eq!(response.header("content-type"), Some("image/jpeg"));
    assert_eq!(
        response.header("cache-control"),
        Some("private, max-age=3600")
    );
}

#[test]
fn what_the_backend_does_not_serve_is_404() {
    let server = TestServer::start(Arc::new(Pictures));
    for path in [
        "/media/obs/git.door43.org~uW~obs_images_360~7146/360px/obs-en-01-02.jpg",
        "/media/obs/another-pack/360px/obs-en-01-01.jpg",
        "/media/obs/git.door43.org~uW~obs_images_360~7146",
        "/media/obs/",
    ] {
        assert_eq!(server.get(path).status, 404, "{path}");
    }
}

#[test]
fn a_backend_with_no_pictures_answers_404() {
    let server = TestServer::start(common::stub());
    assert_eq!(server.get("/media/obs/pack/360px/a.jpg").status, 404);
}

#[test]
fn a_picture_needs_the_token() {
    let server = TestServer::start(Arc::new(Pictures));
    let bare = Req::new(
        "GET",
        "/media/obs/git.door43.org~uW~obs_images_360~7146/360px/obs-en-01-01.jpg",
    )
    .with("Host", &format!("127.0.0.1:{}", server.port));
    assert_eq!(server.send(&bare).status, 401);
}
