//! The Door43 account (unit A8) against the fake Door43 on loopback: sign-in, its cancel in
//! every timing, the kept token, resume, sign-out, the profile link and the session later
//! flows take. A port of tC4's `test/share/signin.test.ts` and `test/share/session.test.ts`,
//! with the rebuild's own rules (APP-DESIGN §5.6 items 5 and 6).

use std::time::Duration;

use serde_json::json;
use tc_api::account::{
    AccountNotice, AccountView, CancelSignIn, KeepState, KeychainState, ProfileLink, Resume,
    ResumeOutcome, ResumeView, SignIn, SignOut,
};
use tc_api::error::SignInReason;
use tc_api::event::Event;
use tc_api::ids::{LocaleId, RequestId};
use tc_api::net::Set;
use tc_api::refusal::SignInRefusal;
use tc_api::test::{Keychain as KeychainHook, KeychainHolds, KeychainMode, KeychainView, Reset};
use tc_api::{ApiError, Endpoint, Refusal, Reply};
use tc_app::account::{Keychain, MemKeychainMode};
use tc_app::events::{Next, Subscription};
use tc_app::prefs::AppDataPath;
use tc_app::reports::{OperationReport, ReportOp, ReportOutcome};
use tc_app::{Answer, App};
use tc_door43::Token;
use tc_door43_fake::seeded::{PASSWORD, USER, USER_EMAIL};
use tc_door43_fake::{AuthSeen, FakeDoor43, FakeUser, Fault, FaultRule, RecordedRequest};

use super::common::{Parts, TestApp, body_of, hello_value, post, rid, token, until};

const SCOPES: [&str; 3] = ["write:repository", "write:organization", "write:user"];

/// A fake with the user of `seeded` (a password outside Latin-1), and nothing else.
async fn fake() -> FakeDoor43 {
    let fake = FakeDoor43::start().await.unwrap();
    fake.add_user(FakeUser {
        login: USER.to_owned(),
        email: USER_EMAIL.to_owned(),
        password: PASSWORD.to_owned(),
        one_time_code: None,
    });
    fake
}

/// An app whose Door43 is `fake`, with the internet on.
async fn app_on(dir: &std::path::Path, fake: &FakeDoor43, parts: &Parts) -> TestApp {
    let url = fake.url().to_owned();
    let t = TestApp::start_with(dir, parts, |config, _| {
        config.door43 = url.parse().unwrap();
    })
    .await;
    internet(&t.app, true).await;
    t
}

async fn internet(app: &App, on: bool) {
    use std::sync::atomic::{AtomicU8, Ordering};
    // A new id each time, so the reply cache never answers an earlier turn with its reply.
    static NEXT: AtomicU8 = AtomicU8::new(0);
    let mut id = [0xA5; 16];
    id[0] = NEXT.fetch_add(1, Ordering::SeqCst);
    post(
        app,
        &Set {
            request: RequestId::from_bytes(id),
            on,
        },
    )
    .await
    .unwrap();
}

fn sign_in_req(id: u8, login: &str, password: &str, stay: bool) -> SignIn {
    SignIn {
        request: rid(id),
        login: login.to_owned(),
        password: password.to_owned(),
        stay,
    }
}

async fn sign_in(app: &App, id: u8, stay: bool) -> Result<AccountView, ApiError> {
    post(app, &sign_in_req(id, USER, PASSWORD, stay)).await
}

async fn state(app: &App) -> AccountView {
    post(app, &tc_api::account::State {}).await.unwrap()
}

async fn cancel(app: &App, id: u8, sign_in: u8) {
    post(
        app,
        &CancelSignIn {
            request: rid(id),
            sign_in: rid(sign_in),
        },
    )
    .await
    .unwrap();
}

async fn resume(app: &App, id: u8) -> Result<ResumeView, ApiError> {
    post(app, &Resume { request: rid(id) }).await
}

async fn sign_out(app: &App, id: u8) -> AccountView {
    post(app, &SignOut { request: rid(id) }).await.unwrap()
}

/// The reply as the server sends it, as text, and as a value.
async fn call_text<E: Endpoint>(app: &App, req: &E) -> (String, Result<E::Ok, ApiError>) {
    let Answer::Reply { body, .. } = app.call(E::PATH, &body_of(req)).await else {
        panic!("no reply")
    };
    let text = String::from_utf8(body.clone()).unwrap();
    let reply: Reply<E::Ok> = serde_json::from_slice(&body).unwrap();
    (text, reply.into_result())
}

fn reports_of(app: &App, op: ReportOp) -> Vec<OperationReport> {
    app.reports()
        .all()
        .into_iter()
        .filter(|r| r.op == op)
        .collect()
}

/// Every report, as text.
fn reports_text(app: &App) -> String {
    format!("{:?}", app.reports().all())
}

/// Every frame a reader has, as text, and the account views among them.
fn frames(events: &mut Subscription) -> (String, Vec<AccountView>) {
    let mut text = String::new();
    let mut accounts = Vec::new();
    while let Some(next) = events.try_next() {
        if let Next::Frame(frame) = next {
            text.push_str(&serde_json::to_string(&frame).unwrap());
            if let Event::Account(view) = frame.event {
                accounts.push(view);
            }
        }
    }
    (text, accounts)
}

fn methods_and_paths(requests: &[RecordedRequest]) -> Vec<(String, String)> {
    requests
        .iter()
        .map(|r| (r.method.clone(), r.path.clone()))
        .collect()
}

async fn until_recorded(fake: &FakeDoor43, count: usize) {
    until("the fake recorded the requests", || {
        fake.requests().len() >= count
    })
    .await;
}

/// Lets tasks the test cannot await finish (a dropped request, a keychain call).
async fn settle_a_while() {
    tokio::time::sleep(Duration::from_millis(150)).await;
}

fn the_keychain(t: &TestApp) -> Option<Token> {
    t.keychain.read().unwrap()
}

fn kept_secret(t: &TestApp) -> Option<String> {
    the_keychain(t).map(|token| token.expose_secret().to_owned())
}

// ---- sign-in -------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sign_in_mints_tc4s_token_and_holds_the_session_in_memory() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let t = app_on(dir.path(), &fake, &Parts::new()).await;
    let reads_at_start = t.keychain.calls().reads;
    let mut events = t.app.events().subscribe(t.app.events().head());

    let view = sign_in(&t.app, 1, false).await.unwrap();
    assert_eq!(view.user.as_deref(), Some(USER));
    assert_eq!(view.kept, KeepState::NotKept);
    // The frame was out before the reply came back.
    let (_, accounts) = frames(&mut events);
    assert_eq!(accounts.last(), Some(&view));
    assert_eq!(state(&t.app).await, view);

    let tokens = fake.tokens_of(USER);
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens[0].name, "translationCore");
    assert_eq!(tokens[0].scopes, SCOPES.map(str::to_owned).to_vec());
    // #366: the keychain is never asked to keep, and an earlier token is forgotten.
    let calls = t.keychain.calls();
    assert_eq!((calls.keeps, calls.forgets), (0, 1));
    assert_eq!(
        calls.reads, reads_at_start,
        "the keychain is read once, at start"
    );
    assert_eq!(the_keychain(&t), None);
    let ok = reports_of(&t.app, ReportOp::Share);
    assert_eq!(ok.len(), 1);
    assert_eq!(ok[0].outcome, ReportOutcome::Ok);
    assert_eq!(ok[0].facts["step"], "sign-in");
    assert_eq!(ok[0].facts["kept"], false);
}

/// #680: the name a project records is Door43's answer, never what was typed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_email_signs_in_as_door43s_username() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let t = app_on(dir.path(), &fake, &Parts::new()).await;
    let view = post(&t.app, &sign_in_req(1, USER_EMAIL, PASSWORD, false))
        .await
        .unwrap();
    assert_eq!(view.user.as_deref(), Some(USER));
    let first = &fake.requests()[0];
    assert_eq!(
        (first.method.as_str(), first.path.as_str()),
        ("GET", "/api/v1/user")
    );
    assert_eq!(
        first.authorization,
        AuthSeen::Basic {
            login: USER_EMAIL.to_owned()
        }
    );
    // The token routes use the username Door43 answered.
    assert!(
        fake.requests()
            .iter()
            .skip(1)
            .all(|r| r.path.starts_with(&format!("/api/v1/users/{USER}/tokens"))),
        "{:?}",
        fake.requests()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stale_translationcore_token_is_replaced_and_another_apps_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    fake.issue_token(USER, "translationCore", &SCOPES).unwrap();
    fake.issue_token(USER, "other-app", &["read:repository"])
        .unwrap();
    let stale = fake.tokens_of(USER)[0].id;
    let t = app_on(dir.path(), &fake, &Parts::new()).await;

    sign_in(&t.app, 1, false).await.unwrap();
    let names = |fake: &FakeDoor43| {
        let mut names: Vec<String> = fake.tokens_of(USER).into_iter().map(|t| t.name).collect();
        names.sort();
        names
    };
    assert_eq!(names(&fake), ["other-app", "translationCore"]);
    let fresh = fake
        .tokens_of(USER)
        .into_iter()
        .find(|t| t.name == "translationCore")
        .unwrap();
    assert_ne!(fresh.id, stale);
    let deletes = |fake: &FakeDoor43| {
        fake.requests()
            .iter()
            .filter(|r| r.method == "DELETE")
            .count()
    };
    assert_eq!(deletes(&fake), 1);
    assert_eq!(
        methods_and_paths(&fake.requests()),
        [
            ("GET", "/api/v1/user".to_owned()),
            // tc-door43 reads a listing until a page comes back short.
            ("GET", format!("/api/v1/users/{USER}/tokens")),
            ("GET", format!("/api/v1/users/{USER}/tokens")),
            ("DELETE", format!("/api/v1/users/{USER}/tokens/{stale}")),
            ("POST", format!("/api/v1/users/{USER}/tokens")),
        ]
        .map(|(m, p)| (m.to_owned(), p))
    );

    // A second sign-in replaces the token the first made.
    sign_in(&t.app, 2, false).await.unwrap();
    assert_eq!(names(&fake), ["other-app", "translationCore"]);
    assert_eq!(deletes(&fake), 2);
}

/// tC4 test 5: each cause has its code, and nothing is stored.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_refusal_names_its_cause_and_stores_nothing() {
    let tokens = format!("/api/v1/users/{USER}/tokens");
    let cases: Vec<(&str, Option<FaultRule>, &str, SignInRefusal)> = vec![
        ("a wrong password", None, "wrong", SignInRefusal::AuthFailed),
        (
            "Door43 down",
            Some(FaultRule::always(
                "GET",
                "/api/v1/user",
                Fault::Status {
                    status: 503,
                    message: "maintenance".into(),
                },
            )),
            PASSWORD,
            SignInRefusal::ServerUnavailable,
        ),
        (
            "a used token name",
            Some(FaultRule::once(
                "POST",
                &tokens,
                Fault::Status {
                    status: 400,
                    message: "access token name has been used already".into(),
                },
            )),
            PASSWORD,
            SignInRefusal::Rejected {
                reason: "access token name has been used already".into(),
            },
        ),
        (
            "a rate limit",
            Some(FaultRule::once(
                "POST",
                &tokens,
                Fault::RateLimited {
                    retry_after_secs: 30,
                },
            )),
            PASSWORD,
            SignInRefusal::Rejected {
                reason: "too many requests".into(),
            },
        ),
    ];
    for (name, fault, password, refusal) in cases {
        let dir = tempfile::tempdir().unwrap();
        let fake = fake().await;
        if let Some(rule) = fault {
            fake.inject(rule);
        }
        let t = app_on(dir.path(), &fake, &Parts::new()).await;
        let calls = t.keychain.calls();
        let (text, reply) = call_text(&t.app, &sign_in_req(1, USER, password, true)).await;
        assert_eq!(
            reply,
            Err(ApiError::Refused(Refusal::SignIn(refusal.clone()))),
            "{name}"
        );
        assert!(
            !text.contains(PASSWORD) && !text.contains("/api/v1"),
            "{name}: {text}"
        );
        assert_eq!(state(&t.app).await.user, None, "{name}");
        assert_eq!(
            t.keychain.calls(),
            calls,
            "{name}: the keychain was not asked"
        );
        let reports = reports_text(&t.app);
        for secret in [USER, PASSWORD, password] {
            assert!(!reports.contains(secret), "{name}: {reports}");
        }
        let refused = reports_of(&t.app, ReportOp::Share);
        assert!(
            matches!(&refused[..], [r] if matches!(&r.outcome, ReportOutcome::Refused { code: Some(_), .. })),
            "{name}: {refused:?}"
        );
    }

    // A Door43 that is not there: a port that was bound, then let go.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_in(dir.path(), |config, _| {
        config.door43 = format!("http://127.0.0.1:{port}").parse().unwrap();
    })
    .await;
    internet(&t.app, true).await;
    assert_eq!(
        sign_in(&t.app, 1, true).await,
        Err(ApiError::Refused(Refusal::SignIn(SignInRefusal::Offline)))
    );
    assert_eq!(state(&t.app).await.user, None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_empty_login_or_password_is_auth_failed_and_sends_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let t = app_on(dir.path(), &fake, &Parts::new()).await;
    let auth_failed = Err(ApiError::Refused(Refusal::SignIn(
        SignInRefusal::AuthFailed,
    )));
    for (id, login, password) in [
        (1, "", PASSWORD),
        (2, "  ", PASSWORD),
        (3, USER, ""),
        (4, "a:b", "x"),
    ] {
        assert_eq!(
            post(&t.app, &sign_in_req(id, login, password, false)).await,
            auth_failed,
            "{login:?} {password:?}"
        );
    }
    assert_eq!(fake.requests(), Vec::new());
    // The login is trimmed; the password is sent as typed.
    let view = post(
        &t.app,
        &sign_in_req(5, &format!("  {USER}\t"), PASSWORD, false),
    )
    .await;
    assert_eq!(view.unwrap().user.as_deref(), Some(USER));
    let padded = post(
        &t.app,
        &sign_in_req(6, USER, &format!(" {PASSWORD}"), false),
    )
    .await;
    assert_eq!(padded, auth_failed, "a password is never trimmed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stay_signed_in_keeps_the_token_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let t = app_on(dir.path(), &fake, &Parts::new()).await;
    let view = sign_in(&t.app, 1, true).await.unwrap();
    assert_eq!(view.kept, KeepState::Kept);
    assert_eq!(view.user.as_deref(), Some(USER));
    assert_eq!(kept_secret(&t), fake.token_secret(USER, "translationCore"));
    let calls = t.keychain.calls();
    assert_eq!((calls.keeps, calls.forgets), (1, 0));
    assert_eq!(reports_of(&t.app, ReportOp::Share)[0].facts["kept"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_keychain_that_refuses_to_keep_is_not_a_failed_sign_in() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let t = app_on(dir.path(), &fake, &Parts::new()).await;
    t.keychain.set_mode(MemKeychainMode::FailingKeep);
    let view = sign_in(&t.app, 1, true).await.unwrap();
    assert_eq!(view.user.as_deref(), Some(USER));
    assert_eq!(view.kept, KeepState::NoKeychain);
    assert_eq!(view.keychain, KeychainState::Works);
    let report = &reports_of(&t.app, ReportOp::Share)[0];
    assert_eq!(report.outcome, ReportOutcome::Ok);
    assert!(report.facts["keepError"].is_string(), "{report:?}");
    assert_eq!(report.facts["kept"], false);

    // A keychain that answers there is none: from now on the step has no "Stay signed in".
    t.keychain.set_mode(MemKeychainMode::NoKeychain);
    let view = sign_in(&t.app, 2, true).await.unwrap();
    assert_eq!(view.kept, KeepState::NoKeychain);
    assert_eq!(view.keychain, KeychainState::Missing);
    assert_eq!(state(&t.app).await.keychain, KeychainState::Missing);
}

/// #366: a sign-in that is not kept clears a token an earlier session kept; a refused
/// password leaves it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sign_in_that_is_not_kept_forgets_an_earlier_token_and_a_refused_password_leaves_it() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let parts = Parts::new();
    parts.keychain.seed(token("earlier-session-token"));
    let t = app_on(dir.path(), &fake, &parts).await;
    assert_eq!(state(&t.app).await.kept, KeepState::Kept);

    let refused = post(&t.app, &sign_in_req(1, USER, "wrong", false)).await;
    assert!(refused.is_err());
    assert_eq!(kept_secret(&t).as_deref(), Some("earlier-session-token"));
    assert_eq!(state(&t.app).await.kept, KeepState::Kept);

    let view = sign_in(&t.app, 2, false).await.unwrap();
    assert_eq!(view.kept, KeepState::NotKept);
    assert_eq!(the_keychain(&t), None);

    let view = sign_in(&t.app, 3, true).await.unwrap();
    assert_eq!(view.kept, KeepState::Kept);
    assert_eq!(kept_secret(&t), fake.token_secret(USER, "translationCore"));
}

/// D85: the password and the token reach nothing the person, a log or another program sees.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_password_and_the_token_never_reach_a_reply_an_event_a_report_or_a_url() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let t = app_on(dir.path(), &fake, &Parts::new()).await;
    let mut events = t.app.events().subscribe(t.app.events().head());
    let mut replies = String::new();

    let (text, signed) = call_text(&t.app, &sign_in_req(1, USER, PASSWORD, true)).await;
    assert!(signed.is_ok());
    replies.push_str(&text);
    let (text, resumed) = call_text(&t.app, &Resume { request: rid(2) }).await;
    assert_eq!(resumed.unwrap().outcome, ResumeOutcome::Resumed);
    replies.push_str(&text);
    replies.push_str(&serde_json::to_string(&hello_value(&t.app).await).unwrap());
    let (text, _) = call_text(&t.app, &ProfileLink {}).await;
    replies.push_str(&text);
    let (text, _) = call_text(&t.app, &SignOut { request: rid(3) }).await;
    replies.push_str(&text);
    let (frames_text, _) = frames(&mut events);
    let reports = reports_text(&t.app);

    let secret = fake.token_secret(USER, "translationCore").unwrap();
    for (what, text) in [
        ("replies", &replies),
        ("events", &frames_text),
        ("reports", &reports),
    ] {
        assert!(!text.contains(PASSWORD), "the password is in the {what}");
        assert!(!text.contains(&secret), "the token is in the {what}");
    }
    for name in [USER, USER_EMAIL] {
        assert!(
            !reports.contains(name),
            "a report names the account: {reports}"
        );
    }
    for request in fake.requests() {
        let url = format!("{}?{}", request.path, request.query.unwrap_or_default());
        assert!(!url.contains(&secret) && !url.contains("pass"), "{url}");
    }
}

/// #120: account calls go to the server the build names, and nowhere else.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn account_calls_go_to_the_door43_server_the_build_names() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let t = app_on(dir.path(), &fake, &Parts::new()).await;
    let host = fake.url().trim_start_matches("http://").to_owned();
    assert_eq!(state(&t.app).await.server, host);
    sign_in(&t.app, 1, false).await.unwrap();
    assert_eq!(
        fake.requests().len(),
        3,
        "the three token calls reached the fake"
    );
    let link = post(&t.app, &ProfileLink {}).await.unwrap();
    assert!(
        link.url.as_str().starts_with(fake.url()),
        "{}",
        link.url.as_str()
    );
}

#[tokio::test]
async fn the_account_names_the_door43_host_it_signs_in_to() {
    for (server, host) in [
        ("https://qa.door43.org", "qa.door43.org"),
        ("https://git.door43.org", "git.door43.org"),
        ("http://127.0.0.1:9", "127.0.0.1:9"),
        ("https://git.door43.org:8443", "git.door43.org:8443"),
    ] {
        let dir = tempfile::tempdir().unwrap();
        let t = TestApp::start_in(dir.path(), |config, _| {
            config.door43 = server.parse().unwrap();
        })
        .await;
        assert_eq!(state(&t.app).await.server, host, "{server}");
    }
}

// ---- cancel ---------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_while_door43_answers_signs_nothing_in_and_keeps_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    fake.inject(FaultRule::once("GET", "/api/v1/user", Fault::Hold));
    let t = app_on(dir.path(), &fake, &Parts::new()).await;
    let app = t.app.clone();
    let signing = tokio::spawn(async move { sign_in(&app, 1, true).await });
    until_recorded(&fake, 1).await;

    cancel(&t.app, 2, 1).await;
    let reply = signing.await.unwrap().unwrap();
    assert_eq!(
        reply.user, None,
        "the cancelled sign-in answers nobody signed in"
    );
    fake.release_held();
    settle_a_while().await;

    let view = state(&t.app).await;
    assert_eq!((view.user, view.kept), (None, KeepState::NotKept));
    assert_eq!(t.keychain.calls().keeps, 0);
    assert_eq!(the_keychain(&t), None);
    assert_eq!(fake.tokens_of(USER), Vec::new());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_that_comes_before_its_sign_in_sends_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let t = app_on(dir.path(), &fake, &Parts::new()).await;
    cancel(&t.app, 9, 5).await;
    let reply = sign_in(&t.app, 5, true).await.unwrap();
    assert_eq!(reply.user, None);
    assert_eq!(fake.requests(), Vec::new());
    // The next sign-in is not cancelled.
    assert_eq!(
        sign_in(&t.app, 6, false).await.unwrap().user.as_deref(),
        Some(USER)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_after_the_sign_in_ended_signs_that_session_out_and_forgets_its_token() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let t = app_on(dir.path(), &fake, &Parts::new()).await;
    assert_eq!(
        sign_in(&t.app, 1, true).await.unwrap().kept,
        KeepState::Kept
    );
    cancel(&t.app, 2, 1).await;
    let view = state(&t.app).await;
    assert_eq!((view.user, view.kept), (None, KeepState::NotKept));
    assert_eq!(the_keychain(&t), None);

    // A cancel that names an older sign-in leaves the newer session as it is.
    sign_in(&t.app, 3, false).await.unwrap();
    sign_in(&t.app, 4, true).await.unwrap();
    cancel(&t.app, 5, 3).await;
    let view = state(&t.app).await;
    assert_eq!(view.user.as_deref(), Some(USER));
    assert_eq!(view.kept, KeepState::Kept);
    assert!(the_keychain(&t).is_some());
}

/// tC4's `signInFlight`: a second sign-in waits for the first, and is never refused as busy.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sign_ins_take_turns_and_the_last_one_keeps_its_session() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    fake.inject(FaultRule::once("GET", "/api/v1/user", Fault::Hold));
    let t = app_on(dir.path(), &fake, &Parts::new()).await;
    let first = tokio::spawn({
        let app = t.app.clone();
        async move { sign_in(&app, 1, false).await }
    });
    until_recorded(&fake, 1).await;
    let second = tokio::spawn({
        let app = t.app.clone();
        async move { sign_in(&app, 2, false).await }
    });
    settle_a_while().await;
    assert_eq!(fake.requests().len(), 1, "the second waits for the first");

    fake.release_held();
    assert_eq!(first.await.unwrap().unwrap().user.as_deref(), Some(USER));
    assert_eq!(second.await.unwrap().unwrap().user.as_deref(), Some(USER));
    let tokens = fake.tokens_of(USER);
    assert_eq!(tokens.len(), 1, "{tokens:?}");
    let session = t.app.door43_session().await.unwrap();
    assert_eq!(
        Some(session.token.expose_secret().to_owned()),
        fake.token_secret(USER, "translationCore")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_switch_turned_off_during_a_sign_in_stops_it_as_offline() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    fake.inject(FaultRule::once("GET", "/api/v1/user", Fault::Hold));
    let t = app_on(dir.path(), &fake, &Parts::new()).await;
    let signing = tokio::spawn({
        let app = t.app.clone();
        async move { sign_in(&app, 1, true).await }
    });
    until_recorded(&fake, 1).await;
    internet(&t.app, false).await;
    fake.release_held();
    assert_eq!(
        signing.await.unwrap(),
        Err(ApiError::Refused(Refusal::SignIn(SignInRefusal::Offline)))
    );
    assert_eq!(state(&t.app).await.user, None);
    assert_eq!(fake.tokens_of(USER), Vec::new());
    assert_eq!(
        fake.requests().len(),
        1,
        "nothing after the switch went off"
    );
}

// ---- resume ---------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_with_a_kept_token_door43_accepts_sets_the_user_and_reads_no_keychain() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let secret = fake.issue_token(USER, "translationCore", &SCOPES).unwrap();
    let parts = Parts::new();
    parts.keychain.seed(token(&secret));
    let t = app_on(dir.path(), &fake, &parts).await;
    assert_eq!(state(&t.app).await.kept, KeepState::Kept);
    assert_eq!(t.keychain.calls().reads, 1);
    assert_eq!(fake.requests(), Vec::new(), "a start sends nothing");

    let resumed = resume(&t.app, 1).await.unwrap();
    assert_eq!(resumed.outcome, ResumeOutcome::Resumed);
    assert_eq!(resumed.account.user.as_deref(), Some(USER));
    assert_eq!(resumed.account.kept, KeepState::Kept);
    let requests = fake.requests();
    assert_eq!(
        methods_and_paths(&requests),
        [("GET".to_owned(), "/api/v1/user".to_owned())]
    );
    assert_eq!(requests[0].authorization, AuthSeen::Token);
    assert_eq!(
        t.keychain.calls().reads,
        1,
        "the keychain is read once, at start"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn resume_with_nothing_kept_asks_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let t = app_on(dir.path(), &fake, &Parts::new()).await;
    let resumed = resume(&t.app, 1).await.unwrap();
    assert_eq!(resumed.outcome, ResumeOutcome::NoneKept);
    assert_eq!(resumed.account.user, None);
    assert_eq!(fake.requests(), Vec::new());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_kept_token_door43_refuses_is_forgotten_with_a_notice() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let parts = Parts::new();
    parts.keychain.seed(token("revoked-token"));
    let t = app_on(dir.path(), &fake, &parts).await;
    let resumed = resume(&t.app, 1).await.unwrap();
    assert_eq!(resumed.outcome, ResumeOutcome::Refused);
    assert_eq!(resumed.account.kept, KeepState::NotKept);
    assert_eq!(resumed.account.notice, Some(AccountNotice::SavedRefused));
    assert_eq!(the_keychain(&t), None);
    // A second check has nothing to ask.
    assert_eq!(
        resume(&t.app, 2).await.unwrap().outcome,
        ResumeOutcome::NoneKept
    );
    assert_eq!(fake.requests().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_answer_keeps_the_token_with_a_notice() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let secret = fake.issue_token(USER, "translationCore", &SCOPES).unwrap();
    fake.inject(FaultRule::once(
        "GET",
        "/api/v1/user",
        Fault::Status {
            status: 503,
            message: "maintenance".into(),
        },
    ));
    let parts = Parts::new();
    parts.keychain.seed(token(&secret));
    let t = app_on(dir.path(), &fake, &parts).await;
    let resumed = resume(&t.app, 1).await.unwrap();
    assert_eq!(resumed.outcome, ResumeOutcome::Unavailable);
    assert_eq!(resumed.account.kept, KeepState::Kept);
    assert_eq!(resumed.account.notice, Some(AccountNotice::SavedUnchecked));
    assert_eq!(kept_secret(&t), Some(secret));
    // The next check tries again, and clears the notice.
    let resumed = resume(&t.app, 2).await.unwrap();
    assert_eq!(resumed.outcome, ResumeOutcome::Resumed);
    assert_eq!(resumed.account.notice, None);
}

/// tC4 session.test: "does not restore a session after Sign out while Door43 replies".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sign_out_while_door43_answers_a_resume_restores_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let secret = fake.issue_token(USER, "translationCore", &SCOPES).unwrap();
    fake.inject(FaultRule::once("GET", "/api/v1/user", Fault::Hold));
    let parts = Parts::new();
    parts.keychain.seed(token(&secret));
    let t = app_on(dir.path(), &fake, &parts).await;
    let resuming = tokio::spawn({
        let app = t.app.clone();
        async move { resume(&app, 1).await }
    });
    until_recorded(&fake, 1).await;
    let out = sign_out(&t.app, 2).await;
    assert_eq!((out.user.clone(), out.kept), (None, KeepState::NotKept));
    fake.release_held();
    let resumed = resuming.await.unwrap().unwrap();
    assert_eq!(resumed.outcome, ResumeOutcome::NoneKept);
    let view = state(&t.app).await;
    assert_eq!((view.user, view.kept), (None, KeepState::NotKept));
    assert_eq!(the_keychain(&t), None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_resumes_share_one_check() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let secret = fake.issue_token(USER, "translationCore", &SCOPES).unwrap();
    fake.inject(FaultRule::once("GET", "/api/v1/user", Fault::Hold));
    let parts = Parts::new();
    parts.keychain.seed(token(&secret));
    let t = app_on(dir.path(), &fake, &parts).await;
    let calls: Vec<_> = (1..=2u8)
        .map(|id| {
            let app = t.app.clone();
            tokio::spawn(async move { resume(&app, id).await })
        })
        .collect();
    until_recorded(&fake, 1).await;
    settle_a_while().await;
    fake.release_held();
    for call in calls {
        assert_eq!(call.await.unwrap().unwrap().outcome, ResumeOutcome::Resumed);
    }
    assert_eq!(fake.requests().len(), 1);
}

// ---- sign-out -------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sign_out_forgets_both_copies_and_sends_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let t = app_on(dir.path(), &fake, &Parts::new()).await;
    sign_in(&t.app, 1, true).await.unwrap();
    fake.clear_requests();
    let mut events = t.app.events().subscribe(t.app.events().head());
    let out = sign_out(&t.app, 2).await;
    assert_eq!(
        (out.user.clone(), out.kept, out.notice),
        (None, KeepState::NotKept, None)
    );
    assert_eq!(frames(&mut events).1.last(), Some(&out));
    assert_eq!(the_keychain(&t), None);
    assert_eq!(fake.requests(), Vec::new());
    assert!(matches!(
        t.app.door43_session().await,
        Err(ApiError::NeedsSignIn(SignInReason::NotSignedIn))
    ));
}

/// internet-consent g: a keychain that cannot forget is said, never hidden.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_keychain_that_cannot_forget_says_so_and_the_sign_in_stays_saved() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let parts = Parts::new();
    parts.keychain.seed(token("kept-token-1"));
    parts.keychain.set_mode(MemKeychainMode::FailingForget);
    let t = app_on(dir.path(), &fake, &parts).await;
    let out = sign_out(&t.app, 1).await;
    assert_eq!(out.user, None);
    assert_eq!(out.kept, KeepState::Kept);
    assert_eq!(out.notice, Some(AccountNotice::SignOutFailed));
    assert_eq!(reports_of(&t.app, ReportOp::Keychain).len(), 1);
    assert_eq!(kept_secret(&t).as_deref(), Some("kept-token-1"));
}

// ---- the profile link and the session of later flows ----------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_profile_link_is_door43s_page_of_the_signed_in_user() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let t = app_on(dir.path(), &fake, &Parts::new()).await;
    assert_eq!(
        post(&t.app, &ProfileLink {}).await,
        Err(ApiError::NeedsSignIn(SignInReason::NotSignedIn))
    );
    sign_in(&t.app, 1, false).await.unwrap();
    fake.clear_requests();
    let link = post(&t.app, &ProfileLink {}).await.unwrap();
    assert_eq!(link.url.as_str(), format!("{}/{USER}", fake.url()));
    assert_eq!(fake.requests(), Vec::new(), "the link sends nothing");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_first_door43_step_resumes_a_kept_token() {
    // Kept and valid: the session, with one request.
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let secret = fake.issue_token(USER, "translationCore", &SCOPES).unwrap();
    let parts = Parts::new();
    parts.keychain.seed(token(&secret));
    let t = app_on(dir.path(), &fake, &parts).await;
    let session = t.app.door43_session().await.unwrap();
    assert_eq!(session.username.as_str(), USER);
    assert_eq!(session.token.expose_secret(), secret);
    assert_eq!(state(&t.app).await.user.as_deref(), Some(USER));
    t.app.door43_session().await.unwrap();
    assert_eq!(fake.requests().len(), 1, "a held session asks nothing");

    // Nothing kept.
    let dir = tempfile::tempdir().unwrap();
    let t = app_on(dir.path(), &fake, &Parts::new()).await;
    assert!(matches!(
        t.app.door43_session().await,
        Err(ApiError::NeedsSignIn(SignInReason::NotSignedIn))
    ));

    // Revoked: forgotten.
    let dir = tempfile::tempdir().unwrap();
    let parts = Parts::new();
    parts.keychain.seed(token("revoked-token"));
    let t = app_on(dir.path(), &fake, &parts).await;
    assert!(matches!(
        t.app.door43_session().await,
        Err(ApiError::NeedsSignIn(SignInReason::TokenRejected))
    ));
    assert_eq!(the_keychain(&t), None);
    assert_eq!(state(&t.app).await.kept, KeepState::NotKept);
}

/// DELTA-10-16 §3.15.1 item 6: `AuthFailed` during a flow forgets both copies.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_session_forgets_both_copies() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let t = app_on(dir.path(), &fake, &Parts::new()).await;
    sign_in(&t.app, 1, true).await.unwrap();
    let session = t.app.door43_session().await.unwrap();
    assert_eq!(
        t.app.door43_refused(&session).await,
        ApiError::NeedsSignIn(SignInReason::TokenRejected)
    );
    let view = state(&t.app).await;
    assert_eq!((view.user, view.kept), (None, KeepState::NotKept));
    assert_eq!(the_keychain(&t), None);

    // A session that is no longer held changes nothing of the one that is.
    sign_in(&t.app, 2, false).await.unwrap();
    t.app.door43_refused(&session).await;
    assert_eq!(state(&t.app).await.user.as_deref(), Some(USER));
}

// ---- no keychain, and the test hooks ----------------------------------------------------------

/// tC4 #496, the server's half: without a keychain, "Stay signed in" cannot work.
#[tokio::test]
async fn no_keychain_hides_stay_signed_in() {
    let dir = tempfile::tempdir().unwrap();
    let parts = Parts::new();
    parts.keychain.set_mode(MemKeychainMode::NoKeychain);
    let t = TestApp::start_with(dir.path(), &parts, |_, _| {}).await;
    let view = state(&t.app).await;
    assert_eq!(view.keychain, KeychainState::Missing);
    assert_eq!(view.kept, KeepState::NotKept);
    assert!(
        reports_of(&t.app, ReportOp::Keychain).is_empty(),
        "not an error"
    );

    let dir = tempfile::tempdir().unwrap();
    let t = TestApp::start_in(dir.path(), |_, _| {}).await;
    assert_eq!(state(&t.app).await.keychain, KeychainState::Works);

    // The product of phase 0 has none until E1a's operating-system keychain.
    let production = tc_app::AppDeps::production();
    assert_eq!(
        production.keychain.read(),
        Err(tc_app::account::KeychainError::NoKeychain)
    );
}

#[tokio::test]
async fn mem_keychain_counts_every_call_and_switches_mode() {
    let keychain = tc_app::account::MemKeychain::new();
    keychain.keep(&token("abc")).unwrap();
    keychain.read().unwrap();
    keychain.set_mode(MemKeychainMode::FailingForget);
    assert!(keychain.forget().is_err());
    keychain.set_mode(MemKeychainMode::FailingKeep);
    assert!(keychain.keep(&token("def")).is_err());
    assert_eq!(keychain.read().unwrap(), Some(token("abc")));
    let calls = keychain.calls();
    assert_eq!((calls.keeps, calls.reads, calls.forgets), (2, 2, 1));
    assert!(keychain.holds());
    keychain.reset();
    assert_eq!(keychain.mode(), MemKeychainMode::Working);
    assert!(!keychain.holds());
    assert_eq!(keychain.calls(), tc_app::account::KeychainCalls::default());
}

async fn set_keychain(
    app: &App,
    id: u8,
    mode: Option<KeychainMode>,
    holds: Option<KeychainHolds>,
    relaunch: bool,
) -> KeychainView {
    post(
        app,
        &KeychainHook {
            request: rid(id),
            mode,
            holds,
            relaunch,
        },
    )
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_keychain_sets_the_keychain_and_reads_the_account_as_a_launch_finds_it() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let t = app_on(dir.path(), &fake, &Parts::new()).await;
    sign_in(&t.app, 1, false).await.unwrap();
    let mut events = t.app.events().subscribe(t.app.events().head());

    let (text, view) = call_text(
        &t.app,
        &KeychainHook {
            request: rid(2),
            mode: None,
            holds: Some(KeychainHolds::Token("kept-by-a-test".into())),
            relaunch: true,
        },
    )
    .await;
    let view = view.unwrap();
    assert!(view.holds);
    assert_eq!(view.reads, 2, "the start's read and the relaunch's");
    assert!(
        !text.contains("kept-by-a-test"),
        "the token never comes back: {text}"
    );
    let (_, accounts) = frames(&mut events);
    let account = state(&t.app).await;
    assert_eq!(accounts.last(), Some(&account), "an Account frame went out");
    assert_eq!((account.user, account.kept), (None, KeepState::Kept));
    assert!(
        !hello_value(&t.app).await["internet"].as_bool().unwrap(),
        "a launch is offline"
    );

    let view = set_keychain(&t.app, 3, Some(KeychainMode::None), None, true).await;
    assert_eq!(view.mode, KeychainMode::None);
    assert_eq!(state(&t.app).await.keychain, KeychainState::Missing);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_reset_empties_the_test_keychain_and_signs_out() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake().await;
    let t = app_on(dir.path(), &fake, &Parts::new()).await;
    set_keychain(&t.app, 1, Some(KeychainMode::FailingForget), None, false).await;
    sign_in(&t.app, 2, true).await.unwrap();
    assert!(t.keychain.holds());

    post(&t.app, &Reset { request: rid(3) }).await.unwrap();
    let view = state(&t.app).await;
    assert_eq!(
        (view.user, view.kept, view.notice),
        (None, KeepState::NotKept, None)
    );
    assert_eq!(view.keychain, KeychainState::Works);
    assert!(!t.keychain.holds());
    assert_eq!(t.keychain.mode(), MemKeychainMode::Working);
    let calls = t.keychain.calls();
    assert_eq!((calls.keeps, calls.forgets), (0, 0));

    // A sign-in held at Door43 does not outlive the reset.
    fake.inject(FaultRule::once("GET", "/api/v1/user", Fault::Hold));
    internet(&t.app, true).await;
    let signing = tokio::spawn({
        let app = t.app.clone();
        async move { sign_in(&app, 4, false).await }
    });
    until_recorded(&fake, 4).await;
    post(&t.app, &Reset { request: rid(5) }).await.unwrap();
    assert_eq!(signing.await.unwrap().unwrap().user, None);
    fake.release_held();
    settle_a_while().await;
    assert_eq!(state(&t.app).await.user, None);
}

/// internet-consent a, the server's half: a choice stored by an older build never turns the
/// switch on at a launch, and a later write keeps the stored keys as they were (§4.9).
#[tokio::test]
async fn a_stored_internet_choice_never_turns_the_switch_on() {
    let dir = tempfile::tempdir().unwrap();
    let parts = Parts::new();
    let stored = json!({"schemaVersion": 1, "internet": true, "askInternet": false});
    parts.files.seed(
        AppDataPath::app_prefs(),
        serde_json::to_vec(&stored).unwrap(),
    );
    let t = TestApp::start_with(dir.path(), &parts, |_, _| {}).await;
    assert_eq!(hello_value(&t.app).await["internet"], false);
    t.app
        .prefs()
        .set_locale(LocaleId::parse("fr").unwrap())
        .await
        .unwrap();
    let written: serde_json::Value =
        serde_json::from_slice(&t.files.get(&AppDataPath::app_prefs()).unwrap()).unwrap();
    assert_eq!(written["internet"], true);
    assert_eq!(written["askInternet"], false);
    assert_eq!(hello_value(&t.app).await["internet"], false);
}
