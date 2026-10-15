//! The fake answers in the swagger's shapes, serves real git archives and exports, and
//! speaks git smart HTTP.

// The helpers sit outside `#[test]` functions, where clippy.toml already allows these.
#![allow(clippy::unwrap_used, clippy::panic)]

use serde_json::Value;
use std::collections::BTreeSet;
use std::io::{Cursor, Read};
use std::path::Path;
use tc_door43_fake::{FakeDoor43, seeded};
use tokio::process::Command;

fn http() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

async fn get(
    fake: &FakeDoor43,
    path: &str,
    token: Option<&str>,
) -> (u16, reqwest::header::HeaderMap, Vec<u8>) {
    let mut request = http().get(format!("{}{path}", fake.url()));
    if let Some(token) = token {
        request = request.header("Authorization", format!("token {token}"));
    }
    let response = request.send().await.unwrap();
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    (status, headers, response.bytes().await.unwrap().to_vec())
}

async fn get_json(fake: &FakeDoor43, path: &str, token: Option<&str>) -> Value {
    let (status, _, body) = get(fake, path, token).await;
    assert_eq!(status, 200, "{path}: {}", String::from_utf8_lossy(&body));
    serde_json::from_slice(&body).unwrap()
}

fn swagger() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/door43/swagger-definitions.json");
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

/// Every object has exactly the properties its swagger definition lists, recursively
/// through `$ref`s.
fn check_shape(
    value: &Value,
    definition: &str,
    defs: &Value,
    at: &str,
    problems: &mut Vec<String>,
) {
    let Some(object) = value.as_object() else {
        problems.push(format!("{at}: expected a {definition} object, got {value}"));
        return;
    };
    let properties = defs[definition]["properties"].as_object().unwrap();
    let have: BTreeSet<&str> = object.keys().map(String::as_str).collect();
    let want: BTreeSet<&str> = properties.keys().map(String::as_str).collect();
    for extra in have.difference(&want) {
        problems.push(format!("{at}: {definition} has no property {extra}"));
    }
    for missing in want.difference(&have) {
        problems.push(format!("{at}: {definition}.{missing} is missing"));
    }
    for (name, schema) in properties {
        let Some(child) = object.get(name).filter(|v| !v.is_null()) else {
            continue;
        };
        let reference = |s: &Value| {
            s["$ref"]
                .as_str()
                .and_then(|r| r.strip_prefix("#/definitions/"))
                .map(str::to_owned)
        };
        if let Some(target) = reference(schema) {
            check_shape(child, &target, defs, &format!("{at}.{name}"), problems);
        } else if let (Some(target), Some(items)) = (reference(&schema["items"]), child.as_array())
        {
            for (i, item) in items.iter().enumerate() {
                check_shape(item, &target, defs, &format!("{at}.{name}[{i}]"), problems);
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn answers_have_exactly_the_swagger_shapes() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let token = fake
        .issue_token(
            seeded::USER,
            "shapes",
            &["write:user", "write:organization"],
        )
        .unwrap();
    let swagger = swagger();
    let defs = &swagger["definitions"];
    let mut problems = Vec::new();
    let tn = "/api/v1/repos/unfoldingWord/en_tn";
    let v86 = fake.resolve("unfoldingWord", "en_tn", "v86").await.unwrap();

    let single = [
        (tn.to_owned(), "Repository"),
        (format!("{tn}/tags/v86"), "Tag"),
        (format!("{tn}/releases/latest"), "Release"),
        (format!("{tn}/git/commits/{v86}"), "Commit"),
        ("/api/v1/user".to_owned(), "User"),
        (
            format!(
                "/api/v1/users/{}/orgs/{}/permissions",
                seeded::USER,
                seeded::TEAM_ORG
            ),
            "OrganizationPermissions",
        ),
        (
            "/api/v1/catalog/entry/unfoldingWord/en_tn/v86".to_owned(),
            "CatalogEntry",
        ),
        (
            "/api/v1/catalog/search?lang=en".to_owned(),
            "CatalogSearchResults",
        ),
    ];
    for (path, definition) in &single {
        let value = get_json(&fake, path, Some(&token)).await;
        check_shape(&value, definition, defs, path, &mut problems);
    }
    let lists = [
        (format!("{tn}/tags"), "Tag"),
        (format!("{tn}/releases"), "Release"),
        ("/api/v1/user/orgs".to_owned(), "Organization"),
    ];
    for (path, definition) in &lists {
        let value = get_json(&fake, path, Some(&token)).await;
        let items = value.as_array().unwrap();
        assert!(!items.is_empty(), "{path}");
        for item in items {
            check_shape(item, definition, defs, path, &mut problems);
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_token_in_the_url_is_refused() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let token = fake.issue_token(seeded::USER, "t", &["read:user"]).unwrap();
    let (status, _, _) = get(&fake, &format!("/api/v1/user?token={token}"), None).await;
    assert_eq!(status, 400);
    let (status, _, _) = get(&fake, &format!("/api/v1/user?access_token={token}"), None).await;
    assert_eq!(status, 400);
    assert_eq!(get(&fake, "/api/v1/user", Some(&token)).await.0, 200);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn listings_clamp_the_page_size_and_report_the_total() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    fake.set_max_page_size(2);
    let (status, headers, body) = get(
        &fake,
        "/api/v1/repos/unfoldingWord/en_tn/tags?limit=50",
        None,
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_slice::<Vec<Value>>(&body).unwrap().len(),
        2
    );
    assert_eq!(headers["x-total-count"], "3");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_archive_is_git_archive_output_with_the_commit_in_its_comment() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let sha = fake.resolve("uW", "en_ugl", "master").await.unwrap();
    let (status, _, body) = get(
        &fake,
        &format!("/api/v1/repos/uW/en_ugl/archive/{sha}.zip"),
        None,
    )
    .await;
    assert_eq!(status, 200);
    let mut zip = zip::ZipArchive::new(Cursor::new(body)).unwrap();
    assert_eq!(zip.comment(), sha.as_bytes());
    let mut names: Vec<String> = (0..zip.len())
        .map(|i| zip.by_index(i).unwrap().name().to_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "en_ugl/",
            "en_ugl/README.md",
            "en_ugl/ingredients/",
            "en_ugl/ingredients/content/",
            "en_ugl/ingredients/content/1.json",
            "en_ugl/metadata.json",
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_burrito_export_declares_the_tag_commit_and_keeps_fixture_bytes() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let v86 = fake.resolve("unfoldingWord", "en_tn", "v86").await.unwrap();
    let (status, _, body) = get(&fake, "/unfoldingWord/en_tn/sb/v86.zip", None).await;
    assert_eq!(status, 200);
    let mut zip = zip::ZipArchive::new(Cursor::new(body)).unwrap();
    let mut read = |name: &str| {
        let mut bytes = Vec::new();
        zip.by_name(name).unwrap().read_to_end(&mut bytes).unwrap();
        bytes
    };
    let metadata: Value = serde_json::from_slice(&read("en_tn/metadata.json")).unwrap();
    assert_eq!(
        metadata["identification"]["primary"]["dcs"]["unfoldingWord/en_tn"]["revision"],
        v86.as_str()
    );
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/tc4/test-fixtures/resources/en_tn@v86/TIT.tsv");
    assert_eq!(
        read("en_tn/ingredients/TIT.tsv"),
        std::fs::read(fixture).unwrap()
    );
    assert!(!read("en_tn/.DS_Store").is_empty());
}

fn git(dir: &Path, home: &Path) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(dir)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.org")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.org")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
    command
}

async fn run(command: &mut Command) -> String {
    let output = command.output().await.unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn git_clones_over_smart_http() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let work = tempfile::tempdir().unwrap();
    run(git(work.path(), work.path()).args([
        "clone",
        "--quiet",
        &fake.clone_url("unfoldingWord", "en_tn"),
        "en_tn",
    ]))
    .await;
    let head = run(git(&work.path().join("en_tn"), work.path()).args(["rev-parse", "HEAD"])).await;
    assert_eq!(
        head,
        fake.resolve("unfoldingWord", "en_tn", "master")
            .await
            .unwrap()
    );
    let tags = run(git(&work.path().join("en_tn"), work.path()).args(["tag", "--list"])).await;
    assert_eq!(tags.lines().collect::<Vec<_>>(), ["v86", "v91", "v92"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn git_pushes_with_a_token_and_is_refused_without_one() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    let token = fake
        .issue_token(seeded::USER, "push", &["write:repository", "write:user"])
        .unwrap();
    let created = http()
        .post(format!("{}/api/v1/user/repos", fake.url()))
        .header("Authorization", format!("token {token}"))
        .header("Content-Type", "application/json")
        .body(r#"{"name":"my-project","auto_init":false,"default_branch":"main"}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(created.status().as_u16(), 201);

    let work = tempfile::tempdir().unwrap();
    let repo = work.path().join("project");
    std::fs::create_dir(&repo).unwrap();
    run(git(&repo, work.path()).args(["init", "--quiet", "--initial-branch", "main"])).await;
    std::fs::write(repo.join("README.md"), "hello\n").unwrap();
    run(git(&repo, work.path()).args(["add", "README.md"])).await;
    run(git(&repo, work.path()).args(["commit", "--quiet", "-m", "first"])).await;
    let local = run(git(&repo, work.path()).args(["rev-parse", "HEAD"])).await;
    let remote = fake.clone_url(seeded::USER, "my-project");

    let refused = git(&repo, work.path())
        .args(["push", "--quiet", &remote, "main"])
        .output()
        .await
        .unwrap();
    assert!(!refused.status.success());

    // The token reaches git through a credential helper, never through the URL.
    let helper = "!f() { echo username=$FAKE_USER; echo password=$FAKE_TOKEN; }; f";
    run(git(&repo, work.path())
        .env("FAKE_USER", seeded::USER)
        .env("FAKE_TOKEN", &token)
        .args([
            "-c",
            "credential.helper=",
            "-c",
            &format!("credential.helper={helper}"),
        ])
        .args(["push", "--quiet", &remote, "main"]))
    .await;
    assert_eq!(
        fake.resolve(seeded::USER, "my-project", "main")
            .await
            .unwrap(),
        local
    );
    assert!(
        fake.requests()
            .iter()
            .all(|r| !r.path.contains(&token)
                && !r.query.clone().unwrap_or_default().contains(&token))
    );
}

/// `git clone` of `owner/name` with `login` and `secret` given by a credential helper.
async fn clone_as(
    fake: &FakeDoor43,
    owner: &str,
    name: &str,
    credentials: Option<(&str, &str)>,
) -> std::process::Output {
    let work = tempfile::tempdir().unwrap();
    let mut command = git(work.path(), work.path());
    if let Some((login, secret)) = credentials {
        let helper = "!f() { echo username=$FAKE_USER; echo password=$FAKE_SECRET; }; f";
        command
            .env("FAKE_USER", login)
            .env("FAKE_SECRET", secret)
            .args([
                "-c",
                "credential.helper=",
                "-c",
                &format!("credential.helper={helper}"),
            ]);
    }
    command
        .args(["clone", "--quiet", &fake.clone_url(owner, name), "copy"])
        .output()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_private_repository_is_cloned_only_by_members_with_a_reading_credential() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    fake.add_user(tc_door43_fake::FakeUser {
        login: "outsider".into(),
        email: "outsider@example.org".into(),
        password: "outsider-pass".into(),
        one_time_code: None,
    });
    fake.add_repo(tc_door43_fake::RepoSeed {
        owner: seeded::TEAM_ORG.into(),
        name: "secret-project".into(),
        subject: String::new(),
        language: "en".into(),
        topics: vec![],
        private: true,
        commits: vec![tc_door43_fake::CommitSeed {
            message: "x".into(),
            files: vec![tc_door43_fake::SeedFile::regular(
                "a.txt",
                b"private".to_vec(),
            )],
            tag: None,
            release: None,
        }],
    })
    .await
    .unwrap();
    let clone = |credentials| clone_as(&fake, seeded::TEAM_ORG, "secret-project", credentials);

    let anonymous = clone(None).await;
    assert!(!anonymous.status.success());

    // To an outsider the repository does not exist, as on Gitea.
    let outsider = clone(Some(("outsider", "outsider-pass"))).await;
    assert!(
        !outsider.status.success(),
        "an outsider cloned a private repository"
    );
    assert!(
        String::from_utf8_lossy(&outsider.stderr).contains("not found"),
        "{}",
        String::from_utf8_lossy(&outsider.stderr)
    );
    let outsider_token = fake
        .issue_token("outsider", "t", &["read:repository"])
        .unwrap();
    let outsider = clone(Some(("outsider", &outsider_token))).await;
    assert!(!outsider.status.success(), "an outsider's token cloned it");

    let member_without_scope = fake
        .issue_token(seeded::USER, "orgs-only", &["read:organization"])
        .unwrap();
    let refused = clone(Some((seeded::USER, &member_without_scope))).await;
    assert!(
        !refused.status.success(),
        "a token without read:repository cloned it"
    );

    let member_token = fake
        .issue_token(seeded::USER, "reader", &["read:repository"])
        .unwrap();
    let member = clone(Some((seeded::USER, &member_token))).await;
    assert!(
        member.status.success(),
        "{}",
        String::from_utf8_lossy(&member.stderr)
    );

    // A public repository stays open to anyone signed in.
    let public = clone_as(
        &fake,
        "unfoldingWord",
        "en_tn",
        Some(("outsider", "outsider-pass")),
    )
    .await;
    assert!(public.status.success());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn raw_and_text_echo_faults_answer_as_given() {
    let fake = FakeDoor43::start_seeded().await.unwrap();
    fake.inject(tc_door43_fake::FaultRule::once(
        "GET",
        "/api/v1/repos/unfoldingWord/en_tn",
        tc_door43_fake::Fault::Raw {
            status: 429,
            headers: vec![("Retry-After".into(), "Wed, 21 Oct 2099 07:28:00 GMT".into())],
            body: b"slow down".to_vec(),
        },
    ));
    let (status, headers, body) = get(&fake, "/api/v1/repos/unfoldingWord/en_tn", None).await;
    assert_eq!(status, 429);
    assert_eq!(headers["retry-after"], "Wed, 21 Oct 2099 07:28:00 GMT");
    assert_eq!(body, b"slow down");

    fake.inject(tc_door43_fake::FaultRule::once(
        "GET",
        "/api/v1/user",
        tc_door43_fake::Fault::EchoAuthorizationAsText { filler: 3 },
    ));
    let (status, headers, body) = get(&fake, "/api/v1/user", Some("abc")).await;
    assert_eq!(status, 400);
    assert!(
        headers["content-type"]
            .to_str()
            .unwrap()
            .starts_with("text/html")
    );
    assert_eq!(body, b"xxxtoken abc");
}
