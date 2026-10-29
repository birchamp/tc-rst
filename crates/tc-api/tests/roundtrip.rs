//! Every wire type reads back as the value that was written (APP-DESIGN §3.1): a property
//! test over generated values of each type, each endpoint's request and each endpoint's
//! reply, errors included. A variant serde cannot write, or reads back as another, fails
//! here rather than in a person's session.
#![cfg(not(target_arch = "wasm32"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code: a failed unwrap is a failed test"
)]

mod common;

use std::fmt::Debug;

use proptest::arbitrary::{Arbitrary, any};
use proptest::test_runner::{Config, TestCaseError, TestRunner};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tc_api::Endpoint;

/// Generated values per type: `TC_ROUNDTRIP_CASES`, 16 by default; the nightly job sets 128. At
/// 128 the two tests took 13 of the gate's test minutes (CI run 40924020433), and the golden
/// examples and the journeys read every endpoint as well.
fn config() -> Config {
    let cases = match std::env::var("TC_ROUNDTRIP_CASES") {
        Err(_) => 16,
        Ok(s) => s
            .parse()
            .unwrap_or_else(|_| panic!("TC_ROUNDTRIP_CASES={s}: a whole number")),
    };
    Config {
        cases,
        failure_persistence: None,
        ..Config::default()
    }
}

fn round_trip<T>() -> Result<(), String>
where
    T: Arbitrary + Serialize + DeserializeOwned + PartialEq + Debug,
{
    let mut runner = TestRunner::new(config());
    runner
        .run(&any::<T>(), |value| {
            let text = serde_json::to_string(&value)
                .map_err(|e| TestCaseError::fail(format!("does not serialize: {e}")))?;
            let back: T = serde_json::from_str(&text)
                .map_err(|e| TestCaseError::fail(format!("{text} does not read back: {e}")))?;
            if back != value {
                return Err(TestCaseError::fail(format!("{text} read back as {back:?}")));
            }
            Ok(())
        })
        .map_err(|e| format!("{}: {e}", std::any::type_name::<T>()))
}

/// Runs `check` on a thread with room for proptest's nested strategies, whose debug-build
/// frames overflow the default test stack for the deeper types.
fn with_big_stack(check: impl FnOnce() -> Vec<Result<(), String>> + Send + 'static) {
    let results = std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(check)
        .unwrap()
        .join()
        .unwrap();
    let failures: Vec<String> = results.into_iter().filter_map(Result::err).collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

macro_rules! types {
    ($($ty:ty),* $(,)?) => {
        vec![$( round_trip::<$ty>() ),*]
    };
}

#[test]
fn every_shared_type_round_trips() {
    use tc_api::account::*;
    use tc_api::align::*;
    use tc_api::app::*;
    use tc_api::buffer::*;
    use tc_api::check::*;
    use tc_api::create::*;
    use tc_api::door43::*;
    use tc_api::error::*;
    use tc_api::event::*;
    use tc_api::feedback::*;
    use tc_api::home::*;
    use tc_api::ids::*;
    use tc_api::jobs::*;
    use tc_api::prefs::*;
    use tc_api::refusal::*;
    use tc_api::shown::*;
    use tc_api::test::*;
    use tc_api::upload::*;
    use tc_api::write::*;
    use tc_api::{NetKind, ReviewToken, ShownToken};

    with_big_stack(|| {
        types![
            // ids and tokens
            ProjectRef,
            ProjectId,
            Epoch,
            ViewSeq,
            JobId,
            PlanId,
            ProposalId,
            CheckSessionId,
            AlignerId,
            ReportId,
            Scope,
            RequestId,
            ServerInstance,
            UploadId,
            DownloadId,
            ConflictRef,
            ChangeRef,
            RecordRef,
            RefPart,
            StoryNo,
            LocaleId,
            WallMs,
            ShownToken,
            ReviewToken,
            // shown values
            Shown<String>,
            ConflictMarker,
            PersonLabel,
            Who,
            Computer,
            Known<u32>,
            Direction,
            tc_api::editor::DraftField,
            tc_api::editor::CommentTarget,
            // writes
            Ack,
            Stored,
            TokenUpdate,
            ConflictBrief,
            SavedValue,
            NotSaved,
            SaveRefusal,
            ReadOnlyReason,
            // errors and refusals
            ApiError,
            SignInReason,
            Flow,
            Feature,
            NetKind,
            Refusal,
            OpenRefusal,
            LockHolder,
            CreateRefusal,
            SettingsRefusal,
            UsfmProblem,
            UsfmVerdict,
            UsfmOutcome,
            ImportRefusal,
            ExportRefusal,
            SyncFailure,
            SyncBlock,
            SyncRefusal,
            ShareRefusal,
            OpenDoor43Refusal,
            SignInRefusal,
            ResourceRefusal,
            ExchangeRefusal,
            FeedbackRefusal,
            PrefsRefusal,
            UploadRefusal,
            UploadFlow,
            UploadView,
            // import
            tc_api::import::ImportReview,
            tc_api::import::VersionsView,
            tc_api::import::UpdateReport,
            // events and jobs
            EventFrame,
            Event,
            SessionEvent,
            SessionMode,
            ChangeCause,
            ChangeScope,
            ChangeParts,
            ChapterRef,
            RecordChange,
            Aspect,
            CheckpointOutcome,
            StartupState,
            StartupProgress,
            JobView,
            JobKind,
            JobState,
            Progress,
            Phase,
            OpenPhase,
            SyncPhase,
            JobOutcome,
            // the states of the views
            Tool,
            CheckUnit,
            CheckListRef,
            CheckPlace,
            Category,
            NoteCategory,
            WordCategory,
            Dot,
            RowFlags,
            RailRow,
            RailRowPatch,
            AlignSave,
            BankSelection,
            Freshness,
            ProjectOrigin,
            Door43Card,
            RepoPlace,
            // the conflict review (B11)
            tc_api::conflicts::ReviewTab,
            tc_api::conflicts::ReviewView,
            tc_api::conflicts::ReviewEntries,
            tc_api::conflicts::ConflictEntry,
            tc_api::conflicts::ConflictKind,
            tc_api::conflicts::ReviewPlace,
            tc_api::conflicts::Version,
            tc_api::conflicts::MadeBy,
            tc_api::conflicts::VersionValue,
            tc_api::conflicts::DetachedEntry,
            tc_api::conflicts::NoticeView,
            tc_api::conflicts::HistoryView,
            tc_api::conflicts::VersionCause,
            // the choices (C7a)
            tc_api::conflicts::KeepChoice,
            tc_api::conflicts::BulkConflict,
            tc_api::conflicts::BulkKept,
            // Home (B2)
            HomeView,
            HomeCard,
            ProjectKind,
            BookTileView,
            StoriesView,
            StoryTileView,
            RecentStory,
            UnusableCard,
            UnusableReason,
            LastEditView,
            ResumePlace,
            VersificationsView,
            LanguagesView,
            LanguageSuggestion,
            // the phase-0 services
            AccountView,
            KeepState,
            KeychainState,
            AccountNotice,
            ResumeOutcome,
            ResumeView,
            LinkView,
            WebUrl,
            ShareTargets,
            OrgChoice,
            RepoSource,
            ProjectName,
            HelloView,
            BuildInfo,
            OpenSession,
            FailedCheckpoint,
            tc_api::session::OutsideEdits,
            tc_api::session::KeptImported,
            SessionRole,
            AppPrefsView,
            DraftUnit,
            PlaceUpdate,
            PlaceChange<RefPart>,
            PlaceChange<u16>,
            PlaceMode,
            FeedbackDraft,
            FeedbackCategory,
            CrashPoint,
            HoldPoint,
            QuietView,
            ValidateView,
            InvalidProject,
            FieldLogView,
            StoredText,
            Echo,
            EchoView,
            EchoWord,
            TokenBase,
            KeychainMode,
            KeychainHolds,
            KeychainView,
            StoredDraft,
            // the draft buffer
            BufferKey,
            FieldKey,
            FieldKind,
            FieldPlace,
            BufferEntry,
            SentSave,
            DraftCall,
            BufferLine,
        ]
    });
}

fn endpoint_round_trips<E>() -> Result<(), String>
where
    E: Endpoint + Arbitrary + PartialEq + Debug,
    E::Ok: Arbitrary + PartialEq + Debug,
{
    round_trip::<E>()?;
    round_trip::<tc_api::ReplyOf<E>>().map_err(|e| format!("{}: {e}", E::PATH))
}

macro_rules! endpoints {
    ($( $kind:ident $([$net:ident])? $path:literal : $req:path => $ok:ty; )*) => {
        vec![$( endpoint_round_trips::<$req>() ),*]
    };
}

#[test]
fn every_request_and_reply_round_trips() {
    with_big_stack(|| {
        let mut results = tc_api::endpoint_table!(endpoints);
        results.extend(tc_api::test_hook_table!(endpoints));
        results
    });
}

/// A generated request of every variant refuses an unknown key in each object it holds, so
/// a variant the golden example does not show cannot read and drop one either (§3.1).
fn refuses_unknown_keys<E>() -> Result<(), String>
where
    E: Endpoint + Arbitrary,
{
    let mut runner = TestRunner::new(config());
    runner
        .run(&any::<E>(), |request| {
            let text = serde_json::to_value(&request).unwrap();
            let accepted =
                common::unknown_key_accepted_in::<E>(&text).map_err(TestCaseError::fail)?;
            if accepted.is_empty() {
                Ok(())
            } else {
                Err(TestCaseError::fail(format!(
                    "{text} reads an unknown key in {accepted:?}"
                )))
            }
        })
        .map_err(|e| format!("{}: {e}", E::PATH))
}

macro_rules! refusals {
    ($( $kind:ident $([$net:ident])? $path:literal : $req:path => $ok:ty; )*) => {
        vec![$( refuses_unknown_keys::<$req>() ),*]
    };
}

#[test]
fn every_generated_request_refuses_an_unknown_key_in_every_object() {
    with_big_stack(|| {
        let mut results = tc_api::endpoint_table!(refusals);
        results.extend(tc_api::test_hook_table!(refusals));
        results
    });
}
