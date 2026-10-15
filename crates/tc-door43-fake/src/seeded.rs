//! The data [`FakeDoor43::start_seeded`] loads.
//!
//! Resource repositories hold real Door43 export files from `fixtures/tc4` (see
//! `fixtures/tc4/test-fixtures/resources/README.md`), laid out as a burrito repository:
//! `metadata.json` at the root, the books under `ingredients/`. The commits, tags and their
//! shas are the fake's own, so each export's `metadata.json` revision is rewritten to the
//! fake's commit when it is served, as DCS writes the commit it exported.
//!
//! The users, organizations and the `v92` pre-release are invented for tests.
//!
//! | Repository | Tags (release kind) | Subject |
//! |---|---|---|
//! | `unfoldingWord/en_tn` | `v86`, `v91` (published), `v92` (pre-release) | TSV Translation Notes |
//! | `unfoldingWord/en_ult` | `v89` (published) | Aligned Bible |
//! | `unfoldingWord/en_twl` | `v86` (published) | TSV Translation Words Links |
//! | `es-419_gl/es-419_tn` | `v66` (published) | TSV Translation Notes |
//! | `uW/en_ugl` | none: pinned by commit only | Greek Lexicon |

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
