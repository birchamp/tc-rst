//! Project settings opened from a Home card (M7, unit B9a; AD14): the values come from the
//! project's derived files, `ingredients/checking/settings.json` and `resources.json`, with no
//! open and no fold, as tC4 reads them (`performOpenSettings`, `loadSettingsGateways`). One
//! server read answers the whole dialog, so its two halves cannot settle apart (tC4 #641).

use serde_json::Value;
use tc_api::ApiError;
use tc_api::jobs::{OpenPhase, Phase};
use tc_api::refusal::SettingsRefusal;
use tc_api::refusal::{OpenRefusal, Refusal};
use tc_api::resources::{FromInstalls, GatewayRef};
use tc_api::settings::{
    CurrentGateway, DEFAULT_FONT, GatewayCard, Save, SettingsView, ShownValues, ViewFromHome,
};
use tc_api::shown::Direction;
use tc_journal::fold::DerivedFile;
use tc_journal::keys::SettingKey;
use tc_journal::{Json, Record, RecordKey};

use crate::app::App;
use crate::backend::{StoreCommand, StoreError};
use crate::dispatch::Handle;
use crate::home::card_facts;
use crate::resources::package;

/// The dialog's own values from `settings.json` (tC4 reads `textDirection`, `textFont` and
/// `languageName`, and nothing else). A file that is missing or holds no object is a project
/// with none of them.
pub fn settings_values(
    settings: Option<&[u8]>,
) -> Result<(String, Direction, Option<String>), String> {
    let doc: Value = match settings {
        None => Value::Null,
        Some(bytes) => serde_json::from_slice(bytes)
            .map_err(|e| format!("{}: {e}", DerivedFile::Settings.path()))?,
    };
    let text = |key: &str| {
        doc.get(key)
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
    };
    let direction = match text("textDirection") {
        Some("rtl") => Direction::Rtl,
        _ => Direction::Ltr,
    };
    Ok((
        text("languageName").unwrap_or_default().to_owned(),
        direction,
        text("textFont").map(str::to_owned),
    ))
}

/// The primary set's gateway language as `resources.json` names it (tC4 `pinnedGateway`): no
/// pins, or a file that does not name one, is no current checking language. A file that does not
/// parse is an error, never "no checking language yet" (tC4 `readJsonSidecarWithMd5` throws).
pub fn pinned_gateway(resources: Option<&[u8]>) -> Result<Option<CurrentGateway>, String> {
    let Some(bytes) = resources else {
        return Ok(None);
    };
    let doc: Value = serde_json::from_slice(bytes)
        .map_err(|e| format!("{}: {e}", DerivedFile::Resources.path()))?;
    let named = || {
        let gateway = doc.pointer("/languageSets/primary/gatewayLanguage")?;
        let language = gateway.get("languageId")?.as_str()?.to_owned();
        let owner = gateway.get("owner")?.as_str()?.to_owned();
        Some(CurrentGateway {
            autonym: tc_resources::gateways::display_name(&language),
            gateway: GatewayRef { language, owner },
        })
    };
    Ok(named())
}

fn unreadable(reason: String) -> ApiError {
    ApiError::Refused(Refusal::Open(OpenRefusal::Unreadable { reason }))
}

impl Handle<ViewFromHome> for App {
    async fn handle(&self, req: ViewFromHome) -> Result<SettingsView, ApiError> {
        let folder = req.project;
        // `session/close` answers before its leave writes the derived files: read them only
        // once that leave, or a close, is over, so a change made just before going Home shows.
        self.inner.sessions.settled(&folder).await;
        let listed = self.inner.home.listed(&folder);
        let looked_for = folder.clone();
        let read = self
            .with_store(move |store| {
                let entry = match listed {
                    Some(entry) => entry,
                    None => store
                        .list()?
                        .projects
                        .into_iter()
                        .find(|e| e.folder == looked_for)
                        .ok_or(StoreError::NotAProject)?,
                };
                let peek = store.peek(&entry.folder)?;
                let settings = store.derived_file(&entry.folder, &DerivedFile::Settings)?;
                let resources = store.derived_file(&entry.folder, &DerivedFile::Resources)?;
                Ok::<_, StoreError>((entry, peek, settings, resources))
            })
            .await?;
        let (entry, peek, settings, resources) = read.map_err(|e| match e {
            StoreError::NotAProject => ApiError::Refused(Refusal::Open(OpenRefusal::NotAProject)),
            error @ (StoreError::Other(_) | StoreError::NotKept) => unreadable(error.to_string()),
        })?;
        // A project with no book yet has none of its derived files (a create before its first
        // book, STORAGE §5.10): its settings and pins are in its history only. The dialog reads
        // them there, through the registry's one open, rather than show a new project's
        // direction and font as the defaults.
        let (settings, resources) = if settings.is_none() && resources.is_none() {
            let lease = self
                .inner
                .sessions
                .open_side(
                    &folder,
                    crate::jobs::ProgressSink::new(Phase::Open(OpenPhase::ReadingChanges)),
                )
                .await
                .map_err(crate::wire::error::open_failure)?;
            let read = lease.session().read();
            let filled = (
                settings.or_else(|| read.render(&DerivedFile::Settings)),
                resources.or_else(|| read.render(&DerivedFile::Resources)),
            );
            drop(lease);
            self.inner.sessions.settled(&folder).await;
            filled
        } else {
            (settings, resources)
        };
        let (language_name, direction, font) =
            settings_values(settings.as_deref()).map_err(unreadable)?;
        let current = pinned_gateway(resources.as_deref()).map_err(unreadable)?;
        let kind = crate::home::api_kind(entry.kind);
        let books = card_facts(&peek.metadata, &entry.folder, kind)
            .books
            .unwrap_or_default();
        let options = match self.installed_pins().await {
            Ok(installed) => FromInstalls::Listed(
                tc_resources::gateway_change::gateways_covering_project(
                    &installed,
                    &self.inner.resources,
                    &books,
                    crate::resources::sets_kind(entry.kind),
                )
                .into_iter()
                .map(package)
                .collect(),
            ),
            Err(reason) => FromInstalls::Failed { reason },
        };
        Ok(SettingsView {
            language_name,
            direction,
            font,
            gateway: GatewayCard { current, options },
        })
    }
}

/// The text of a top-level `settings.json` key as the fold projects it.
fn setting_text(read: &crate::counters::CountingSnapshot, key: &str) -> Option<String> {
    let key = RecordKey::Setting(SettingKey::parse(key).ok()?);
    match read.projected(&key) {
        Some(Record::Setting {
            value: Some(Json::Str(text)),
            ..
        }) if !text.is_empty() => Some(text),
        _ => None,
    }
}

impl Handle<Save> for App {
    /// Save changes from a Home card's Project settings (AD14): the project opens through the
    /// registry's one open of the folder, so a card clicked meanwhile shares the session
    /// (AD38); what the dialog showed must still be what the project holds, or nothing is
    /// saved; then each setting that changed is a batch of its own (R18's fallback), and the
    /// refusal says which was written.
    async fn handle(&self, req: Save) -> Result<(), ApiError> {
        let lease = self
            .inner
            .sessions
            .open_side(
                &req.project,
                crate::jobs::ProgressSink::new(Phase::Open(OpenPhase::ReadingChanges)),
            )
            .await
            .map_err(crate::wire::error::open_failure)?;
        let outcome = save_settings(lease.session(), &req).await;
        drop(lease);
        self.inner.sessions.settled(&req.project).await;
        self.inner.bus.publish(tc_api::event::Event::HomeCard {
            project: req.project,
        });
        outcome
    }
}

async fn save_settings(
    session: &crate::session::ProjectSession,
    req: &Save,
) -> Result<(), ApiError> {
    let refused = |refusal| ApiError::Refused(Refusal::Settings(refusal));
    let handle = session.handle();
    // One snapshot decides: the equality check and both tokens are of the same fold, so a
    // token names only heads the dialog's values came from.
    let read = session.read();
    let held = ShownValues {
        direction: match setting_text(&read, "textDirection").as_deref() {
            Some("rtl") => Direction::Rtl,
            _ => Direction::Ltr,
        },
        font: setting_text(&read, "textFont"),
    };
    if held != req.shown {
        return Err(refused(SettingsRefusal::Changed));
    }
    let direction_changed = req.direction != req.shown.direction;
    let font_changed = match &req.shown.font {
        Some(font) => *font != req.font,
        None => req.font != DEFAULT_FONT,
    };
    let write = |key: &'static str, text: String| {
        let key = SettingKey::parse(key).map_err(|e| e.to_string())?;
        let token = read
            .mint(&RecordKey::Setting(key), None)
            .map_err(|e| e.to_string())?;
        Ok::<_, String>(
            StoreCommand::set_setting(token, Some(Json::Str(text))).requested_by(req.request),
        )
    };
    let direction_text = match req.direction {
        Direction::Ltr => "ltr",
        Direction::Rtl => "rtl",
    };
    if direction_changed {
        let command = write("textDirection", direction_text.to_owned())
            .map_err(|reason| refused(SettingsRefusal::DirectionNotSaved { reason }))?;
        handle.submit(command).await.map_err(|error| {
            refused(SettingsRefusal::DirectionNotSaved {
                reason: error.to_string(),
            })
        })?;
    }
    if font_changed {
        let command = write("textFont", req.font.clone()).map_err(|reason| {
            refused(SettingsRefusal::FontNotSaved {
                reason,
                direction_saved: direction_changed,
            })
        })?;
        handle.submit(command).await.map_err(|error| {
            refused(SettingsRefusal::FontNotSaved {
                reason: error.to_string(),
                direction_saved: direction_changed,
            })
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_with_no_settings_shows_tc4s_defaults() {
        assert_eq!(
            settings_values(None).unwrap(),
            (String::new(), Direction::Ltr, None)
        );
        assert_eq!(
            settings_values(Some(br#"{"schemaVersion":1,"textDirection":"sideways"}"#)).unwrap(),
            (String::new(), Direction::Ltr, None)
        );
    }

    #[test]
    fn an_obsolete_checking_language_is_never_read() {
        let settings = r#"{"schemaVersion":1,"checkingLanguage":"en","textDirection":"rtl",
            "textFont":"Charis SIL","languageName":"Español"}"#;
        assert_eq!(
            settings_values(Some(settings.as_bytes())).unwrap(),
            ("Español".into(), Direction::Rtl, Some("Charis SIL".into()))
        );
        let pins = br#"{"schemaVersion":2,"languageSets":{"primary":{"gatewayLanguage":
            {"languageId":"es-419","owner":"es-419_gl"}}}}"#;
        let current = pinned_gateway(Some(pins)).unwrap().unwrap();
        assert_eq!(current.autonym, "Español (Latinoamérica)");
        assert_eq!(current.gateway.key(), "es-419::es-419_gl");
    }

    #[test]
    fn no_pins_is_no_checking_language_never_english() {
        assert_eq!(pinned_gateway(None), Ok(None));
        assert_eq!(pinned_gateway(Some(br#"{"schemaVersion":2}"#)), Ok(None));
    }

    #[test]
    fn a_language_the_registry_does_not_name_shows_its_tag() {
        let pins = br#"{"languageSets":{"primary":{"gatewayLanguage":
            {"languageId":"xx","owner":"someone"}}}}"#;
        assert_eq!(pinned_gateway(Some(pins)).unwrap().unwrap().autonym, "xx");
    }
}
