//! One value of every refusal variant. `assert_every_variant` checks each list against the
//! variants serde knows, so a new variant fails the tests until it has a sample here.

use tc_api::refusal::{
    CreateRefusal, ExchangeRefusal, ExportRefusal, FeedbackRefusal, ImportRefusal,
    OpenDoor43Refusal, OpenRefusal, PrefsRefusal, Refusal, ResourceRefusal, ShareRefusal,
    SignInRefusal, SyncFailure, SyncRefusal,
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
    ]
}

pub fn create() -> Vec<CreateRefusal> {
    vec![
        CreateRefusal::NameInUse { abbr: s("kr_ulb") },
        CreateRefusal::Failed {
            reason: s("No space left on device (os error 28)"),
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
        ExportRefusal::PrintFailed,
        ExportRefusal::PrintFailedPictures,
        ExportRefusal::NothingDrafted,
    ]
}

pub fn sync_refusals() -> Vec<SyncRefusal> {
    vec![
        SyncRefusal::Offline,
        SyncRefusal::NotSignedIn,
        SyncRefusal::AuthFailed,
        SyncRefusal::NotShared,
        SyncRefusal::NoWriteAccess {
            server_message: s("user does not have write access"),
        },
        SyncRefusal::RefRejected {
            server_message: s("protected branch hook declined"),
        },
        SyncRefusal::Busy { attempts: 3 },
        SyncRefusal::RemoteMissing,
        SyncRefusal::Unreachable,
        SyncRefusal::ProjectMismatch,
        SyncRefusal::NeedsNewerApp {
            major: 2,
            min_reader_minor: 0,
        },
        SyncRefusal::Tc4TeamRepository,
        SyncRefusal::ReadOnly,
        SyncRefusal::Io {
            reason: s("No space left on device (os error 28)"),
        },
    ]
}

/// Every sync refusal, before and after the received work was kept.
pub fn sync() -> Vec<SyncFailure> {
    sync_refusals()
        .into_iter()
        .flat_map(|refusal| {
            [false, true].map(|after_intake| SyncFailure {
                refusal: refusal.clone(),
                after_intake,
            })
        })
        .collect()
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
        ResourceRefusal::GatewayFailed,
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

/// Every refusal of every family.
pub fn refusals() -> Vec<Refusal> {
    let mut all = Vec::new();
    all.extend(open().into_iter().map(Refusal::Open));
    all.extend(create().into_iter().map(Refusal::Create));
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
    all
}
