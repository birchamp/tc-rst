//! The App language dialog's rules (M16): tC4's guards.

use tc_api::ids::LocaleId;
use tc_ui::app::language::LanguageDialog;

fn id(s: &str) -> LocaleId {
    LocaleId::parse(s).unwrap()
}

#[test]
fn opening_shows_the_applied_language_and_apply_is_off() {
    let (dialog, show) = LanguageDialog::open(id("es-419"));
    assert_eq!(show, id("es-419"));
    assert_eq!(dialog.preview(), id("es-419"));
    assert!(!dialog.apply_enabled());
    assert!(!dialog.locked() && !dialog.error());
}

#[test]
fn a_selection_previews_and_enables_apply() {
    let (mut dialog, _) = LanguageDialog::open(LocaleId::ENGLISH);
    assert_eq!(dialog.select(id("fr")), Some(id("fr")));
    assert!(dialog.apply_enabled());
    // The language already previewed changes nothing.
    assert_eq!(dialog.select(id("fr")), None);
    // Back to the applied one: Apply is off again.
    assert_eq!(dialog.select(LocaleId::ENGLISH), Some(LocaleId::ENGLISH));
    assert!(!dialog.apply_enabled());
    assert_eq!(dialog.start_apply(), None);
}

#[test]
fn cancel_shows_the_applied_language() {
    let (mut dialog, _) = LanguageDialog::open(id("ar"));
    dialog.select(id("hi"));
    assert_eq!(dialog.cancel(), Some(id("ar")));
}

/// tC4 withdraws the select, Apply, Cancel, the ✕, Escape and the scrim while the save runs.
/// The select's own guard is what makes the design system's Select, which takes a row click
/// while disabled (TC4-ISSUES §1 row 13), harmless here.
#[test]
fn nothing_changes_while_the_save_runs() {
    let (mut dialog, _) = LanguageDialog::open(LocaleId::ENGLISH);
    dialog.select(id("fr"));
    assert_eq!(dialog.start_apply(), Some(id("fr")));
    assert!(dialog.locked());
    assert!(!dialog.apply_enabled());
    assert_eq!(dialog.select(id("vi")), None);
    assert_eq!(dialog.preview(), id("fr"));
    assert_eq!(dialog.cancel(), None);
    assert_eq!(dialog.start_apply(), None);
}

#[test]
fn a_failed_save_keeps_the_preview_and_allows_a_retry() {
    let (mut dialog, _) = LanguageDialog::open(LocaleId::ENGLISH);
    dialog.select(id("uk"));
    dialog.start_apply();
    dialog.apply_failed();
    assert!(dialog.error());
    assert!(!dialog.locked());
    assert_eq!(dialog.preview(), id("uk"));
    assert!(dialog.apply_enabled());
    assert_eq!(dialog.start_apply(), Some(id("uk")));
    assert!(!dialog.error());
    dialog.apply_failed();
    // A new selection clears the error; Cancel restores the applied language.
    dialog.select(id("ru"));
    assert!(!dialog.error());
    assert_eq!(dialog.cancel(), Some(LocaleId::ENGLISH));
    assert_eq!(dialog.applied(), LocaleId::ENGLISH);
}
