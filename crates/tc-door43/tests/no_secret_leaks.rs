//! A password or token never reaches a URL, a log line, or the text of a value or error.
//!
//! This file holds one test because it installs a process-wide log subscriber that
//! records everything, including the `log` records of reqwest and its dependencies.

// The capture writer sits outside the `#[test]` function, where clippy.toml already
// allows unwrap.
#![allow(clippy::unwrap_used)]

mod common;

use base64::Engine;
use common::{client, credentials, repo, token_name};
use std::io::Write;
use std::sync::{Arc, Mutex};
use tc_door43::{NewRepoOwner, Token, TokenScope, Visibility};
use tc_door43_fake::{AuthSeen, FakeDoor43, Fault, FaultRule, seeded};
use tracing_subscriber::util::SubscriberInitExt;

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Captured {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn secrets_stay_out_of_urls_logs_and_debug_output() {
    let logs = Captured::default();
    let writer = logs.clone();
    tracing_log::LogTracer::init().unwrap();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish()
        .try_init()
        .unwrap();

    let fake = FakeDoor43::start_seeded().await.unwrap();
    let client = client(&fake);
    let mut shown = Vec::new();

    let session = client
        .mint_token(&credentials(), &token_name(), &TokenScope::FOR_SHARING)
        .await
        .unwrap();
    let token = session.token.expose_secret().to_owned();
    shown.push(format!("{client:?} {session:?} {:?}", credentials()));
    client.list_orgs(&session).await.unwrap();
    let created = client
        .create_repo(
            &session,
            &NewRepoOwner::SignedInUser,
            &"leak-check".parse().unwrap(),
            Visibility::Public,
        )
        .await
        .unwrap();
    shown.push(format!("{created:?}"));
    client.repo(&created.repo, Some(&session)).await.unwrap();

    // A server that repeats the Authorization header in its error message.
    fake.inject(FaultRule::once(
        "GET",
        "/api/v1/user/orgs",
        Fault::EchoAuthorization,
    ));
    let echoed = client.list_orgs(&session).await.unwrap_err();
    shown.push(format!("{echoed} {echoed:?}"));
    assert!(format!("{echoed}").contains("<redacted>"), "{echoed}");
    fake.inject(FaultRule::once(
        "GET",
        "/api/v1/user",
        Fault::EchoAuthorization,
    ));
    let echoed_basic = client
        .mint_token(&credentials(), &token_name(), &TokenScope::FOR_SHARING)
        .await
        .unwrap_err();
    shown.push(format!("{echoed_basic} {echoed_basic:?}"));

    // A proxy that echoes the header as text, putting the secret across the point where
    // an error message is cut.
    for filler in [150, 175, 180, 185, 190, 194, 195, 199, 200, 205] {
        fake.inject(FaultRule::once(
            "GET",
            "/api/v1/user/orgs",
            Fault::EchoAuthorizationAsText { filler },
        ));
        let echoed = client.list_orgs(&session).await.unwrap_err();
        shown.push(format!("{echoed} {echoed:?}"));
        fake.inject(FaultRule::once(
            "GET",
            "/api/v1/user",
            Fault::EchoAuthorizationAsText { filler },
        ));
        let echoed = client
            .mint_token(&credentials(), &token_name(), &TokenScope::FOR_SHARING)
            .await
            .unwrap_err();
        assert!(format!("{echoed}").contains("xxxx"), "{echoed}");
        shown.push(format!("{echoed} {echoed:?}"));
    }

    let revoked = client
        .session_for_token(Token::new("0000-revoked-token-0000".to_owned()).unwrap())
        .await
        .unwrap_err();
    shown.push(format!("{revoked} {revoked:?}"));
    let forbidden = client
        .create_repo(
            &session,
            &NewRepoOwner::Organization(seeded::READ_ONLY_ORG.parse().unwrap()),
            &"leak-check".parse().unwrap(),
            Visibility::Public,
        )
        .await
        .unwrap_err();
    shown.push(format!("{forbidden} {forbidden:?}"));
    let _ = client
        .repo(&repo("unfoldingWord/en_tn"), Some(&session))
        .await;

    let basic = base64::engine::general_purpose::STANDARD.encode(format!(
        "{}:{}",
        seeded::USER,
        seeded::PASSWORD
    ));
    let secrets = [
        token.as_str(),
        seeded::PASSWORD,
        basic.as_str(),
        "0000-revoked-token-0000",
    ];
    let logged = logs.text();
    assert!(
        logged.contains("door43 request"),
        "the log capture saw nothing"
    );
    assert!(logged.contains("/api/v1/user/orgs"));
    let shown = shown.join("\n");
    for secret in secrets {
        assert!(
            !logged.contains(secret),
            "a secret reached the log:\n{logged}"
        );
        // Not even the start of one: a cut base64 value still decodes to part of the
        // password.
        for (end, _) in secret.char_indices().skip(8) {
            assert!(
                !shown.contains(&secret[..end]),
                "part of a secret ({}) reached Debug or Display:\n{shown}",
                &secret[..end]
            );
        }
        assert!(
            !shown.contains(secret),
            "a secret reached Debug or Display:\n{shown}"
        );
    }

    let requests = fake.requests();
    for request in &requests {
        let url = format!(
            "{}?{}",
            request.path,
            request.query.clone().unwrap_or_default()
        );
        for secret in secrets {
            assert!(!url.contains(secret), "a secret reached a URL: {url}");
        }
    }
    // The credentials did travel, in the header.
    assert!(requests.iter().any(|r| r.authorization
        == AuthSeen::Basic {
            login: seeded::USER.to_owned()
        }));
    assert!(requests.iter().any(|r| r.authorization == AuthSeen::Token));
}
