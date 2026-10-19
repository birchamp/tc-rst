//! `feedback/compose` (APP-DESIGN §3.9, §4.7, Q-2): the attachment the Feedback dialog shows and
//! the shell sends, as tC4's `attachmentText` builds it (`tC4:src/data/feedback.ts:24-32`).
//!
//! The text is English whatever the app language, as tC4's is (#731): the help desk reads it.
//! It holds the version line and the report of an operation, never a token (`reports.rs`).

use serde_json::{Map, Value, json};
use tc_api::app::BuildInfo;
use tc_api::ids::WallMs;

use crate::reports::{OperationReport, ReportOutcome};

/// The operating system and the processor this server was built for: `Linux x86_64`,
/// `Windows x86_64`, `macOS aarch64`. tC4 sent `navigator.platform`, which says `MacIntel` on
/// Apple silicon and `Win32` on 64-bit Windows (#731), so the help desk could not tell them.
pub fn platform() -> String {
    let os = match std::env::consts::OS {
        "linux" => "Linux",
        "windows" => "Windows",
        "macos" => "macOS",
        other => other,
    };
    format!("{os} {}", std::env::consts::ARCH)
}

/// tC4's `${APP_LINE} · ${platform}`: `translationCore {version} ({commit}) · {platform}`.
pub fn version_line(build: &BuildInfo, platform: &str) -> String {
    format!(
        "translationCore {} ({}) · {platform}",
        build.version, build.commit
    )
}

/// The version line, then `no Report`, or the report labelled with its operation and its end,
/// then the report as JSON indented by two spaces (`JSON.stringify(report, null, 2)`).
pub fn attachment_text(version_line: &str, report: Option<&OperationReport>) -> String {
    let Some(report) = report else {
        return format!("{version_line}\nno Report\n");
    };
    let json = report_json(report);
    let op = json.get("op").and_then(Value::as_str).unwrap_or_default();
    // A map of strings, numbers and booleans always serializes.
    let pretty = serde_json::to_string_pretty(&json).unwrap_or_default();
    format!(
        "{version_line}\nReport: {op}, {}\n{pretty}\n",
        iso_utc_millis(report.ended_at)
    )
}

/// tC4's `Report` shape (`tC4:src/data/journal/runtime.ts:261-272`), in its key order: `op`,
/// `ok`, `code` when a refusal has one, `facts`, `startedAt`, `endedAt`. tC4's `rule` has no
/// counterpart here; the rebuild adds `key` (the refusal's sentence) and `reason` (what failed
/// without a refusal), which tC4 put nowhere the help desk could read.
fn report_json(report: &OperationReport) -> Value {
    let mut out = Map::new();
    out.insert("op".to_owned(), json!(report.op));
    out.insert(
        "ok".to_owned(),
        json!(matches!(report.outcome, ReportOutcome::Ok)),
    );
    match &report.outcome {
        ReportOutcome::Ok => {}
        ReportOutcome::Refused { code, key } => {
            if let Some(code) = code {
                out.insert("code".to_owned(), json!(code));
            }
            out.insert("key".to_owned(), json!(key));
        }
        ReportOutcome::Failed { reason } => {
            out.insert("reason".to_owned(), json!(reason));
        }
    }
    out.insert("facts".to_owned(), Value::Object(report.facts.clone()));
    out.insert(
        "startedAt".to_owned(),
        json!(iso_utc_millis(report.started_at)),
    );
    out.insert("endedAt".to_owned(), json!(iso_utc_millis(report.ended_at)));
    Value::Object(out)
}

/// JavaScript's `new Date(at).toISOString()`: `2026-10-19T12:34:56.789Z`.
pub fn iso_utc_millis(at: WallMs) -> String {
    let millis = at.0.rem_euclid(1000);
    let seconds = at.0.div_euclid(1000);
    let (days, rest) = (seconds.div_euclid(86_400), seconds.rem_euclid(86_400));
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{millis:03}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// The date of a day count since 1970-01-01 (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}
