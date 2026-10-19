//! Project settings' and Get source texts' forms (B9a): tC4's `test/settingsOpen.test.ts`
//! cases that one server read leaves (#641: a reply lands only on its own project's form, a
//! failed read keeps Save disabled), and the rows of a package.

use tc_api::home::{BookTileView, Door43Card, HomeCard, ProjectKind};
use tc_api::ids::ProjectRef;
use tc_api::resources::{
    FromInstalls, GatewayPackage, GatewayRef, PackageRole, PackageRow, Seen, SourcesView,
};
use tc_api::settings::{CurrentGateway, GatewayCard, SettingsView};
use tc_api::shown::{Direction, Known};
use tc_core::BookId;
use tc_ui::app::sources::{GatewayState, SettingsForm, SourcesForm};

fn card(project: &str, books: &[BookId]) -> HomeCard {
    HomeCard {
        project: ProjectRef::parse(project).unwrap(),
        name: format!("{project} name"),
        tag: Known::Known("ar".into()),
        dir: Direction::Ltr,
        kind: ProjectKind::Bible,
        door43: Door43Card::Local,
        books: Known::Known(
            books
                .iter()
                .map(|book| BookTileView {
                    book: *book,
                    drafted: Known::Unknown,
                    conflicts: 0,
                })
                .collect(),
        ),
        stories: None,
        conflicts: Known::Unknown,
    }
}

fn gateway(language: &str, owner: &str) -> GatewayRef {
    GatewayRef {
        language: language.into(),
        owner: owner.into(),
    }
}

fn view() -> SettingsView {
    SettingsView {
        language_name: "العربية".into(),
        direction: Direction::Rtl,
        font: Some("Scheherazade New — Arabic script".into()),
        gateway: GatewayCard {
            current: Some(CurrentGateway {
                gateway: gateway("ar", "BSOJ"),
                autonym: "العربية".into(),
            }),
            options: FromInstalls::Listed(Vec::new()),
        },
    }
}

#[test]
fn settings_open_with_the_cards_facts_and_save_waits_for_the_read() {
    let mut st = SettingsForm::of(&card("alef", &[BookId::Titus]));
    assert_eq!((st.name.as_str(), st.code.as_str()), ("alef name", "ar"));
    assert_eq!(st.font, "Noto Sans (default)");
    assert!(st.one_book());
    assert_eq!(st.gateway, GatewayState::Loading);
    assert!(
        st.save_disabled(),
        "the defaults are never saved over the project's values"
    );
    let alef = st.project.clone();
    st.read(&alef, view());
    assert!(!st.save_disabled());
    assert_eq!(st.dir, Direction::Rtl);
    assert_eq!(st.font, "Scheherazade New — Arabic script");
    assert_eq!(st.lang_name, "العربية");
    assert!(matches!(st.gateway, GatewayState::Read(_)));
}

#[test]
fn a_reply_for_another_projects_settings_changes_nothing() {
    // settingsOpen 5 and 6: Settings closed, or another project's opened, before the read came.
    let mut st = SettingsForm::of(&card("bet", &[BookId::Titus, BookId::Jonah]));
    let before = st.clone();
    let alef = ProjectRef::parse("alef").unwrap();
    st.read(&alef, view());
    st.failed(&alef, "gone".into());
    assert_eq!(st, before);
    assert!(!st.one_book());
}

#[test]
fn a_failed_read_keeps_save_disabled_and_states_why() {
    // settingsOpen 4 and 7: the reason shows, Save stays disabled, the card leaves its
    // loading text.
    let mut st = SettingsForm::of(&card("alef", &[BookId::Titus]));
    let alef = st.project.clone();
    st.failed(&alef, "settings.json: the file is held".into());
    assert!(st.save_disabled());
    assert_eq!(st.error.as_deref(), Some("settings.json: the file is held"));
    assert_eq!(st.gateway, GatewayState::Failed);
}

fn package(language: &str, owner: &str) -> GatewayPackage {
    GatewayPackage {
        gateway: gateway(language, owner),
        name: "English".into(),
        autonym: "English".into(),
        direction: Direction::Ltr,
        seen: Seen {
            tn: "v89".into(),
            tw: "v89".into(),
            ta: "v89".into(),
        },
    }
}

fn row(role: PackageRole, repo: &str) -> PackageRow {
    PackageRow {
        role,
        repo: repo.into(),
        desc: String::new(),
    }
}

#[test]
fn the_text_is_always_included_and_helps_can_be_left_out() {
    let mut src = SourcesForm::default();
    let en = package("en", "unfoldingWord");
    src.pick(en.clone());
    let rows = vec![
        row(PackageRole::Text, "en_ult"),
        row(PackageRole::Notes, "en_tn"),
    ];
    src.rows_read(&en, BookId::Titus, rows.clone());
    assert!(src.can_download());
    src.toggle(&rows[0]);
    src.toggle(&rows[1]);
    assert_eq!(src.chosen(), ["en_ult"]);
    src.toggle(&rows[1]);
    assert_eq!(src.chosen(), ["en_ult", "en_tn"]);
}

#[test]
fn rows_for_a_package_or_book_left_meanwhile_are_not_shown() {
    let mut src = SourcesForm::default();
    let (en, es) = (
        package("en", "unfoldingWord"),
        package("es-419", "es-419_gl"),
    );
    src.pick(en.clone());
    src.pick(es.clone());
    src.rows_read(&en, BookId::Titus, vec![row(PackageRole::Notes, "en_tn")]);
    assert!(src.loading && src.rows.is_empty());
    src.set_book(BookId::Jonah);
    src.rows_failed(&es, BookId::Titus, "late".into());
    assert_eq!(src.error, None);
    src.rows_read(
        &es,
        BookId::Jonah,
        vec![row(PackageRole::Notes, "es-419_tn")],
    );
    assert!(!src.loading);
    assert_eq!(src.chosen(), ["es-419_tn"]);
    src.change_gateway();
    assert_eq!(src.gateway, None);
    assert!(src.rows.is_empty());
}

#[test]
fn the_current_package_compares_its_owner_without_letter_case() {
    let mut src = SourcesForm {
        packages: Some(SourcesView {
            gateways: Vec::new(),
            current: Some(gateway("es-419", "ES-419_GL")),
            checkable: FromInstalls::Listed(vec![gateway("en", "unfoldingWord")]),
            pinned: true,
        }),
        ..SourcesForm::default()
    };
    assert!(src.is_current(&package("es-419", "es-419_gl")));
    assert!(!src.is_current(&package("en", "unfoldingWord")));
    assert!(src.is_checkable(&package("en", "unfoldingWord")));
    src.not_read();
    assert!(src.needs_internet);
}
