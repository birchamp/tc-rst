//! One value of every refusal variant. `assert_every_variant` checks each list against the
//! variants serde knows, so a new variant fails the tests until it has a sample here.

use tc_api::refusal::{
    CreateRefusal, ExchangeRefusal, ExportRefusal, FeedbackRefusal, ImportRefusal, LockHolder,
    OpenDoor43Refusal, OpenRefusal, PrefsRefusal, Refusal, ResourceRefusal, SettingsRefusal,
    ShareRefusal, SignInRefusal, SyncBlock, SyncFailure, SyncRefusal, UploadRefusal,
};

fn s(text: &str) -> String {
    text.to_owned()
}

pub fn open() -> Vec<OpenRefusal> {
    vec![
        OpenRefusal::NewerFormat { format: 2 },
        OpenRefusal::NotAProject,
        OpenRefusal::Unreadable {
            reason: s("Access is denied. (os error 5)"),
        },
        OpenRefusal::LockHeld {
            holder: Some(LockHolder {
                host: s("DESKTOP-7Q2"),
                pid: 4242,
            }),
        },
        OpenRefusal::LockHeld { holder: None },
    ]
}

pub fn create() -> Vec<CreateRefusal> {
    vec![
        CreateRefusal::NameInUse { abbr: s("kr_ulb") },
        CreateRefusal::NameRequired,
        CreateRefusal::CodeRequired,
        CreateRefusal::AbbrRequired,
        CreateRefusal::CodeNotHeld {
            code: s("acm-x-lebarabic"),
            reason: s("a language tag part is longer than eight letters"),
        },
        CreateRefusal::Failed {
            reason: s("No space left on device (os error 28)"),
        },
        CreateRefusal::CopyUnsafe {
            reason: tc_api::write::ReadOnlyReason::CloudSynced,
        },
    ]
}

pub fn settings() -> Vec<SettingsRefusal> {
    vec![
        SettingsRefusal::Changed,
        SettingsRefusal::DirectionNotSaved {
            reason: s("No space left on device (os error 28)"),
        },
        SettingsRefusal::FontNotSaved {
            reason: s("No space left on device (os error 28)"),
            direction_saved: false,
        },
        SettingsRefusal::FontNotSaved {
            reason: s("No space left on device (os error 28)"),
            direction_saved: true,
        },
    ]
}

pub fn import() -> Vec<ImportRefusal> {
    vec![ImportRefusal::WriteFailed, ImportRefusal::NameExists]
}

pub fn export() -> Vec<ExportRefusal> {
    vec![
        ExportRefusal::CheckpointFailed,
        ExportRefusal::ReadFailed,
        ExportRefusal::WriteFailed,
        ExportRefusal::PrintFailed,
        ExportRefusal::PrintFailedPictures,
        ExportRefusal::NothingDrafted,
    ]
}

pub fn sync_blocks() -> Vec<SyncBlock> {
    vec![
        SyncBlock::NotShared,
        SyncBlock::ProjectMismatch,
        SyncBlock::NeedsNewerApp {
            major: 2,
            min_reader_minor: 0,
        },
        SyncBlock::Tc4TeamRepository,
    ]
}

pub fn sync_refusals() -> Vec<SyncRefusal> {
    vec![
        SyncRefusal::Offline,
        SyncRefusal::NotSignedIn,
        SyncRefusal::AuthFailed,
        SyncRefusal::NoWriteAccess {
            server_message: s("user does not have write access"),
        },
        SyncRefusal::RefRejected {
            server_message: s("protected branch hook declined"),
        },
        SyncRefusal::Busy { attempts: 3 },
        SyncRefusal::RemoteMissing,
        SyncRefusal::Unreachable,
        SyncRefusal::ReadOnly,
        SyncRefusal::Io {
            reason: s("No space left on device (os error 28)"),
        },
    ]
}

/// Every failure a sync can end with: each block, and each step's refusal before and after
/// the received work was kept.
pub fn sync() -> Vec<SyncFailure> {
    let mut all: Vec<SyncFailure> = sync_blocks()
        .into_iter()
        .map(SyncFailure::Blocked)
        .collect();
    all.extend(sync_refusals().into_iter().map(SyncFailure::BeforeIntake));
    all.extend(sync_refusals().into_iter().map(SyncFailure::AfterIntake));
    all
}

pub fn share() -> Vec<ShareRefusal> {
    vec![
        ShareRefusal::NameExists,
        ShareRefusal::Offline,
        ShareRefusal::AuthFailed,
        ShareRefusal::CreateRejected {
            server_message: s("repository quota exceeded"),
        },
        ShareRefusal::PushFailed {
            reason: s("the connection was reset"),
        },
    ]
}

pub fn open_door43() -> Vec<OpenDoor43Refusal> {
    vec![
        OpenDoor43Refusal::NameExists,
        OpenDoor43Refusal::NoJournal,
        OpenDoor43Refusal::Removed,
    ]
}

pub fn sign_in() -> Vec<SignInRefusal> {
    vec![
        SignInRefusal::AuthFailed,
        SignInRefusal::Offline,
        SignInRefusal::ServerUnavailable,
        SignInRefusal::Rejected {
            reason: s("a token with this name exists"),
        },
    ]
}

pub fn resources() -> Vec<ResourceRefusal> {
    vec![
        ResourceRefusal::CatalogUnreadable {
            reason: s("the catalog answered 502"),
        },
        ResourceRefusal::SomeFailed {
            list: s("en_ult, en_tn"),
        },
        ResourceRefusal::FixFailed {
            reason: s("the archive is damaged"),
        },
        ResourceRefusal::UpdateCheckFailed {
            reason: s("the catalog answered 502"),
        },
        ResourceRefusal::UpgradeFailed {
            reason: s("the archive is damaged"),
        },
        ResourceRefusal::GatewayFailed {
            reason: "the notes of Titus could not be read".to_owned(),
        },
        ResourceRefusal::GatewayBlocked {
            books: s("Titus, Ruth"),
        },
        ResourceRefusal::AdoptFailed {
            error: s("resources.json is read-only"),
        },
    ]
}

pub fn exchange() -> Vec<ExchangeRefusal> {
    vec![
        ExchangeRefusal::Damaged,
        ExchangeRefusal::NewerFormat { format: 2 },
        ExchangeRefusal::VerifyFailed,
        ExchangeRefusal::NotWritten {
            reason: s("the drive is full"),
        },
        ExchangeRefusal::Unreadable {
            reason: s("the drive was removed"),
        },
    ]
}

pub fn feedback() -> Vec<FeedbackRefusal> {
    vec![
        FeedbackRefusal::NoDesktop,
        FeedbackRefusal::NotConfigured,
        FeedbackRefusal::Offline,
        FeedbackRefusal::Timeout,
        FeedbackRefusal::Refused,
    ]
}

pub fn prefs() -> Vec<PrefsRefusal> {
    vec![PrefsRefusal::LocaleNotSaved]
}

pub fn upload() -> Vec<UploadRefusal> {
    vec![UploadRefusal::NotWritten {
        reason: s("No space left on device (os error 28)"),
    }]
}

/// Every refusal of every family.
pub fn refusals() -> Vec<Refusal> {
    let mut all = Vec::new();
    all.extend(open().into_iter().map(Refusal::Open));
    all.extend(create().into_iter().map(Refusal::Create));
    all.extend(settings().into_iter().map(Refusal::Settings));
    all.extend(import().into_iter().map(Refusal::Import));
    all.extend(export().into_iter().map(Refusal::Export));
    all.extend(sync().into_iter().map(Refusal::Sync));
    all.extend(share().into_iter().map(Refusal::Share));
    all.extend(open_door43().into_iter().map(Refusal::OpenDoor43));
    all.extend(sign_in().into_iter().map(Refusal::SignIn));
    all.extend(resources().into_iter().map(Refusal::Resources));
    all.extend(exchange().into_iter().map(Refusal::Exchange));
    all.extend(feedback().into_iter().map(Refusal::Feedback));
    all.extend(prefs().into_iter().map(Refusal::Prefs));
    all.extend(upload().into_iter().map(Refusal::Upload));
    all
}
