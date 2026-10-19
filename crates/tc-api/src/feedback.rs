//! The `feedback` service. The Feedback dialog (M14) shows the report it will send; the
//! shell sends it, after the UI asked the internet question and only while the server's
//! internet switch is on (owner's answer to Q-2). The server only composes the attachment
//! from its report log (`tC4:src/data/feedback.ts`).

use serde::{Deserialize, Serialize};

use crate::ids::ReportId;

/// `feedback/compose`: the attachment for a new report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Compose {
    /// The report of the refused operation "Ask for help" was pressed on. Without one, the
    /// last finished operation's report is attached, as tC4's `reportToAttach` does.
    pub report: Option<ReportId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct FeedbackDraft {
    /// The version line and the report, exactly as the dialog shows it and the shell sends
    /// it (`attachmentText`).
    pub attachment: String,
}

/// tC3's three categories; the help desk's subject is `tC: <category>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum FeedbackCategory {
    General,
    Content,
    Bug,
}

impl FeedbackCategory {
    /// The category as the help desk receives it (`CATEGORIES` in tC4).
    pub fn subject_name(self) -> &'static str {
        match self {
            Self::General => "General Feedback",
            Self::Content => "Content and Resources Feedback",
            Self::Bug => "Bug Report",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::General => "feedback.category.general",
            Self::Content => "feedback.category.content",
            Self::Bug => "feedback.category.bug",
        }
    }
}
