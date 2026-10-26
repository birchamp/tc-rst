//! The data [`FakeDoor43::start_seeded`] loads.
//!
//! Resource repositories hold real Door43 export files from `fixtures/tc4` (see
//! `fixtures/tc4/test-fixtures/resources/README.md`), laid out as a burrito repository:
//! `metadata.json` at the root, the books under `ingredients/`. The commits, tags and their
//! shas are the fake's own, so each export's `metadata.json` revision is rewritten to the
//! fake's commit when it is served, as DCS writes the commit it exported.
//!
//! The users, organizations and the `v92` pre-release are invented for tests.
//! [`parity_account`] is the one account the parity captures sign in with.
//!
//! | Repository | Tags (release kind) | Subject |
//! |---|---|---|
//! | `unfoldingWord/en_tn` | `v86`, `v87`, `v91` (published), `v92` (pre-release) | TSV Translation Notes |
//! | `unfoldingWord/en_ult` | `v89` (published) | Aligned Bible |
//! | `unfoldingWord/en_twl` | `v86` (published) | TSV Translation Words Links |
//! | `es-419_gl/es-419_tn` | `v66` (published) | TSV Translation Notes |
//! | `uW/en_ugl` | none: pinned by commit only | Greek Lexicon |
//!
//! `en_tn` `v87` is the version tC4's tC3 import fixtures name (`TC3_DCS_TAGS`, tC4 j09 :280 and
//! :321); it holds `v91`'s files, since an import pins it and downloads nothing. The fake binary
//! also holds the Greek (`el-x-koine_ugnt` `v0.21`, `v0.34`) and Hebrew (`hbo_uhb` `v2.1.30`)
//! tags those fixtures name ([`tc3_original_languages`]).

use crate::{
    CommitSeed, FakeDoor43, FakeError, FakeOrg, FakeUser, OrgMember, ReleaseKind, RepoSeed,
    SeedFile,
};
use std::path::PathBuf;

/// A user with a password outside Latin-1, to catch Basic auth that is not UTF-8.
pub const USER: &str = "translator";
pub const USER_EMAIL: &str = "translator@example.org";
pub const PASSWORD: &str = "pässwörd ✓ correct-horse";
/// A user whose sign-in also needs [`OTP_CODE`].
pub const OTP_USER: &str = "guarded";
pub const OTP_PASSWORD: &str = "guarded-password";
pub const OTP_CODE: &str = "424242";
/// [`USER`] may create repositories here.
pub const TEAM_ORG: &str = "Kaleo-Translators";
/// [`USER`] is a member here but may not create repositories.
pub const READ_ONLY_ORG: &str = "es-419_gl";

/// The account of the parity captures and the app's browser tests: the user of tC4's own e2e
/// fake (`parity/tc4-baseline/mock-server/fake-door43.mjs`, `FAKE_USER`), so the two fakes
/// sign in the same person. A test reads the `.mjs` and checks that these values agree.
pub const PARITY_USER: &str = "facilitator-zq";
pub const PARITY_PASSWORD: &str = "a pass-word";
pub const PARITY_EMAIL: &str = "facilitator-zq@example.org";
/// The parity user's organizations, as `FAKE_ORGANIZATIONS` lists them: the name, the full
/// name, whether the user may create a repository there, and how many repositories it holds in
/// the language of the parity projects, [`PARITY_LANGUAGE`] (R12: the share dialog marks the
/// organization with the most of them that the user may create in as Recommended).
pub const PARITY_ORGS: [(&str, &str, bool, usize); 2] = [
    ("Equipo-Ejemplo", "Equipo Ejemplo", true, 4),
    ("es-419_gl", "Latin American Spanish Gateway", false, 12),
];

/// The language of the parity samples, whose repositories [`PARITY_ORGS`] counts.
pub const PARITY_LANGUAGE: &str = "es-419";

/// Adds the parity user, the organizations and their repositories in [`PARITY_LANGUAGE`] (empty,
/// public). Use it on a fake made with [`FakeDoor43::start`]: [`FakeDoor43::start_seeded`]
/// already has an `es-419_gl`.
pub async fn parity_account(fake: &FakeDoor43) -> Result<(), FakeError> {
    fake.add_user(FakeUser {
        login: PARITY_USER.to_owned(),
        email: PARITY_EMAIL.to_owned(),
        password: PARITY_PASSWORD.to_owned(),
        one_time_code: None,
    });
    for (name, full_name, can_create_repository, repositories) in PARITY_ORGS {
        fake.add_org(FakeOrg {
            name: name.to_owned(),
            full_name: full_name.to_owned(),
            members: vec![OrgMember {
                login: PARITY_USER.to_owned(),
                can_create_repository,
            }],
        });
        for n in 1..=repositories {
            fake.add_repo(RepoSeed {
                owner: name.to_owned(),
                name: format!("{PARITY_LANGUAGE}_project_{n}"),
                subject: "Bible".to_owned(),
                language: PARITY_LANGUAGE.to_owned(),
                topics: Vec::new(),
                private: false,
                commits: Vec::new(),
            })
            .await?;
        }
    }
    Ok(())
}

fn resources_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tc4/test-fixtures/resources")
}

fn read(dir: &str, file: &str) -> Result<Vec<u8>, FakeError> {
    let path = resources_dir().join(dir).join(file);
    std::fs::read(&path).map_err(|e| FakeError::Invalid(format!("fixture {}: {e}", path.display())))
}

/// A snapshot from a fixture folder: `metadata.json` plus each listed file under
/// `ingredients/`, the paths the export's metadata names.
fn burrito(dir: &str, ingredients: &[(&str, &str)]) -> Result<Vec<SeedFile>, FakeError> {
    let mut files = vec![SeedFile::regular(
        "metadata.json",
        read(dir, "metadata.json")?,
    )];
    for (fixture_file, ingredient) in ingredients {
        files.push(SeedFile::regular(
            &format!("ingredients/{ingredient}"),
            read(dir, fixture_file)?,
        ));
    }
    Ok(files)
}

fn release(tag: &str, kind: ReleaseKind, files: Vec<SeedFile>) -> CommitSeed {
    CommitSeed {
        message: format!("Release {tag}"),
        files,
        tag: Some(tag.to_owned()),
        release: Some(kind),
    }
}

fn resource(
    owner: &str,
    name: &str,
    subject: &str,
    language: &str,
    commits: Vec<CommitSeed>,
) -> RepoSeed {
    RepoSeed {
        owner: owner.to_owned(),
        name: name.to_owned(),
        subject: subject.to_owned(),
        language: language.to_owned(),
        topics: vec!["tc-ready".to_owned()],
        private: false,
        commits,
    }
}

pub(crate) async fn seed(fake: &FakeDoor43) -> Result<(), FakeError> {
    fake.add_user(FakeUser {
        login: USER.to_owned(),
        email: USER_EMAIL.to_owned(),
        password: PASSWORD.to_owned(),
        one_time_code: None,
    });
    fake.add_user(FakeUser {
        login: OTP_USER.to_owned(),
        email: "guarded@example.org".to_owned(),
        password: OTP_PASSWORD.to_owned(),
        one_time_code: Some(OTP_CODE.to_owned()),
    });
    fake.add_org(FakeOrg {
        name: TEAM_ORG.to_owned(),
        full_name: "Kaleo Translation Team".to_owned(),
        members: vec![OrgMember {
            login: USER.to_owned(),
            can_create_repository: true,
        }],
    });
    fake.add_org(FakeOrg {
        name: READ_ONLY_ORG.to_owned(),
        full_name: "Español Latinoamericano".to_owned(),
        members: vec![OrgMember {
            login: USER.to_owned(),
            can_create_repository: false,
        }],
    });

    unfolding_word_resources(fake).await?;
    fake.add_repo(resource(
        READ_ONLY_ORG,
        "es-419_tn",
        "TSV Translation Notes",
        "es-419",
        vec![release(
            "v66",
            ReleaseKind::Published,
            burrito("es-419_tn@v66", &[("TIT.tsv", "TIT.tsv")])?,
        )],
    ))
    .await?;
    fake.add_repo(RepoSeed {
        topics: Vec::new(),
        ..resource(
            "uW",
            "en_ugl",
            "Greek Lexicon",
            "en",
            vec![CommitSeed {
                message: "Greek lexicon".to_owned(),
                files: {
                    let mut files =
                        burrito("en_ugl@d9d29e2d5892", &[("1.json", "content/1.json")])?;
                    files.push(SeedFile::regular(
                        "README.md",
                        read("en_ugl@d9d29e2d5892", "README.md")?,
                    ));
                    files
                },
                tag: None,
                release: None,
            }],
        )
    })
    .await?;
    Ok(())
}

/// unfoldingWord's resource repositories of the table above (`en_tn`, `en_ult`, `en_twl`), the
/// ones a gateway-language package of English is read from. [`FakeDoor43::start_seeded`] holds
/// them, and so does the fake binary beside [`parity_account`], so the app's browser tests and
/// its Rust tests download and upgrade from one set of releases.
pub async fn unfolding_word_resources(fake: &FakeDoor43) -> Result<(), FakeError> {
    let tn_v91 = burrito("en_tn@v91", &[("TIT.tsv", "TIT.tsv")])?;
    fake.add_repo(resource(
        "unfoldingWord",
        "en_tn",
        "TSV Translation Notes",
        "en",
        vec![
            release(
                "v86",
                ReleaseKind::Published,
                burrito(
                    "en_tn@v86",
                    &[("TIT.tsv", "TIT.tsv"), ("JON.tsv", "JON.tsv")],
                )?,
            ),
            release("v87", ReleaseKind::Published, tn_v91.clone()),
            release("v91", ReleaseKind::Published, tn_v91.clone()),
            release("v92", ReleaseKind::PreRelease, tn_v91),
        ],
    ))
    .await?;
    fake.add_repo(resource(
        "unfoldingWord",
        "en_ult",
        "Aligned Bible",
        "en",
        vec![release(
            "v89",
            ReleaseKind::Published,
            burrito("en_ult@v89", &[("TIT.usfm", "TIT.usfm")])?,
        )],
    ))
    .await?;
    fake.add_repo(resource(
        "unfoldingWord",
        "en_twl",
        "TSV Translation Words Links",
        "en",
        vec![release(
            "v86",
            ReleaseKind::Published,
            burrito("en_twl@v86", &[("TIT.tsv", "TIT.tsv")])?,
        )],
    ))
    .await?;
    Ok(())
}

/// The original-language tags tC4's tC3 import fixtures name beside `en_tn` `v87`
/// (`TC3_DCS_TAGS`, tC4 j09 :280 and :321), for the fake binary the browser journeys use. Tags
/// with no release, as the lookup reads tags only: the catalog stays the seeded fake's. The
/// Rust tests seed their own Greek releases, so [`FakeDoor43::start_seeded`] leaves these out.
pub async fn tc3_original_languages(fake: &FakeDoor43) -> Result<(), FakeError> {
    for (name, subject, tags) in [
        (
            "el-x-koine_ugnt",
            "Greek New Testament",
            &["v0.21", "v0.34"][..],
        ),
        ("hbo_uhb", "Hebrew Old Testament", &["v2.1.30"][..]),
    ] {
        let commits = tags
            .iter()
            .map(|tag| {
                Ok(CommitSeed {
                    message: format!("Tag {tag}"),
                    files: burrito("en_ult@v89", &[("TIT.usfm", "TIT.usfm")])?,
                    tag: Some((*tag).to_owned()),
                    release: None,
                })
            })
            .collect::<Result<Vec<_>, FakeError>>()?;
        fake.add_repo(resource("unfoldingWord", name, subject, "en", commits))
            .await?;
    }
    Ok(())
}
