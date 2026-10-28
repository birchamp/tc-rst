//! Uploads (APP-DESIGN §5.5, AD26): `POST /api/v1/uploads` with the file as the raw request
//! body, one file each, streamed to the server's `uploads/` folder. The file's name and the
//! flow it belongs to travel in two headers, because the body is the file.

use serde::{Deserialize, Serialize};

use crate::ids::{RequestId, UploadId};

/// `POST`, the upload route.
pub const UPLOADS_PATH: &str = "/api/v1/uploads";

/// The file's name, UTF-8 and percent-encoded. It names the file for the person and never a
/// path: the server stores the bytes under the upload's id.
pub const UPLOAD_NAME_HEADER: &str = "X-Tc-Upload-Name";

/// The flow the file is for, [`UploadFlow::header_value`].
pub const UPLOAD_FLOW_HEADER: &str = "X-Tc-Upload-Flow";

/// The flows that upload a file. Each has its own size cap (A-28).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum UploadFlow {
    /// "Receive work from a file" in a browser: an exchange file, which holds a whole
    /// project's work.
    Exchange,
    /// A tC3 or tC4 project, or a Scripture Burrito.
    Import,
    /// A USFM file added to a project.
    Usfm,
    /// The file a guided fix takes.
    GuidedFix,
}

impl UploadFlow {
    pub const ALL: [UploadFlow; 4] = [Self::Exchange, Self::Import, Self::Usfm, Self::GuidedFix];

    /// The most bytes an upload for this flow may carry. An exchange file may be 4 GiB,
    /// STORAGE's cap for the whole file, whose 512 MiB cap applies to each entry inside it
    /// and is checked when the file is read; every other upload is 512 MiB. A size equal to
    /// the cap is allowed.
    pub const fn cap(self) -> u64 {
        match self {
            Self::Exchange => 4 * 1024 * 1024 * 1024,
            Self::Import | Self::Usfm | Self::GuidedFix => 512 * 1024 * 1024,
        }
    }

    /// The text of [`UPLOAD_FLOW_HEADER`]: the flow's name on the wire.
    pub const fn header_value(self) -> &'static str {
        match self {
            Self::Exchange => "exchange",
            Self::Import => "import",
            Self::Usfm => "usfm",
            Self::GuidedFix => "guidedFix",
        }
    }

    /// The flow a header names, or `None` for any other text.
    pub fn from_header(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|flow| flow.header_value() == value)
    }
}

/// What the server answers when the whole file is stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct UploadView {
    pub id: UploadId,
    /// The name the person's file had.
    pub name: String,
    /// The bytes stored.
    pub size: u64,
}

/// `uploads/drop`: the person's dialog no longer has a use for an upload (it closed, its row
/// was removed, or another file took its place), so the server deletes it now instead of at
/// the sweep. An upload a job already took, or one that is gone, is left alone: the answer is
/// the same.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DropUpload {
    pub request: RequestId,
    pub upload: UploadId,
}
