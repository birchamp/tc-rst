//! Project settings opened from a Home card (M7, unit B9a; AD14): the values come from the
//! project's derived files, `ingredients/checking/settings.json` and `resources.json`, with no
//! open and no fold, as tC4 reads them (`performOpenSettings`, `loadSettingsGateways`). One
//! server read answers the whole dialog, so its two halves cannot settle apart (tC4 #641).

use serde_json::Value;
use tc_api::ApiError;
use tc_api::refusal::{OpenRefusal, Refusal};
use tc_api::resources::{FromInstalls, GatewayRef};
use tc_api::settings::{CurrentGateway, GatewayCard, SettingsView, ViewFromHome};
use tc_api::shown::Direction;
use tc_journal::fold::DerivedFile;

use crate::app::App;
use crate::backend::StoreError;
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
        let listed = self.inner.home.listed(&folder);
        let read = self
            .with_store(move |store| {
                let entry = match listed {
                    Some(entry) => entry,
                    None => store
                        .list()?
                        .projects
                        .into_iter()
                        .find(|e| e.folder == folder)
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
            StoreError::Other(reason) => unreadable(reason),
        })?;
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
