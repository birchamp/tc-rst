//! The one rule for a project's text direction (INV-TEXT-DIRECTION-HAS-ONE-SOURCE, tC4
//! `projectPresentation`, `src/state.jsx` 2144-2159): the metadata's
//! `languages[0].scriptDirection`, else the settings file's `textDirection`. Settings writes
//! the field it reads first, so Home, the open project and Align agree after a change.

use tc_api::shown::Direction;
use tc_journal::keys::{MetaField, SettingKey};
use tc_journal::{Json, Record, RecordKey};

use crate::counters::CountingSnapshot;

pub fn project_direction(
    script_direction: Option<&str>,
    text_direction: Option<&str>,
) -> Direction {
    match script_direction.or(text_direction) {
        Some("rtl") => Direction::Rtl,
        _ => Direction::Ltr,
    }
}

/// `languages[0].scriptDirection` of a `metadata.json` document.
pub fn script_direction(metadata: &serde_json::Value) -> Option<&str> {
    metadata
        .pointer("/languages/0/scriptDirection")
        .and_then(serde_json::Value::as_str)
}

/// The direction of the open project, from the fold's metadata and settings records.
pub fn of_fold(read: &CountingSnapshot) -> Direction {
    let script = match read.projected(&RecordKey::ProjectMeta(MetaField::Languages)) {
        Some(Record::ProjectMeta {
            value: Some(Json::Arr(languages)),
            ..
        }) => languages
            .first()
            .and_then(Json::as_obj)
            .and_then(|language| language.get("scriptDirection"))
            .and_then(Json::as_str)
            .map(str::to_owned),
        _ => None,
    };
    let text = SettingKey::parse("textDirection").ok().and_then(|key| {
        match read.projected(&RecordKey::Setting(key)) {
            Some(Record::Setting {
                value: Some(Json::Str(text)),
                ..
            }) => Some(text),
            _ => None,
        }
    });
    project_direction(script.as_deref(), text.as_deref())
}
