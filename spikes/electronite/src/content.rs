//! What the stand-in serves: the probe page, the help page, OBS-sized pictures, a download,
//! a print document and the aligner's 20 KB reply.

use std::sync::OnceLock;

pub const INDEX_HTML: &str = include_str!("../page/index.html");
pub const PROBE_JS: &str = include_str!("../page/probe.js");
pub const PROBE_CSS: &str = include_str!("../page/probe.css");
pub const HELP_HTML: &str = include_str!("../page/help.html");
pub const HELP_CSS: &str = include_str!("../page/help.css");
pub const CHILD_HTML: &str = include_str!("../page/child.html");
pub const PRINT_HTML: &str = include_str!("../page/print.html");
pub const PREPARE_HTML: &str = "<!doctype html><meta charset=utf-8><title>Preparing</title>\
<p id=prepare-ok>Preparing resources: 0 of 0</p>";

/// A web font of the kind tC4 serves (Charis SIL, SIL Open Font License).
pub const PROBE_FONT: &[u8] = include_bytes!(
    "../../../crates/tc-ui/assets/ds/assets/fonts/charis-sil-latin-400-normal.woff2"
);
/// A Graphite-only font (it has `Silf` and no `GSUB`): with Graphite its letters join; without
/// it they stand alone. The page's Graphite probe is informational.
pub const AWAMI_FONT: &[u8] =
    include_bytes!("../../../crates/tc-ui/assets/ds/assets/fonts/AwamiNastaliq-Regular.ttf");

/// The aligner reply the round trip asks for: 20 KB, as AD18 and §8.3 say.
pub const REPLY_BYTES: usize = 20 * 1024;

/// Width and height of an OBS picture (tC4 ships 640×360 JPEGs of 50 to 90 KB).
pub const PICTURE_WIDTH: u32 = 640;
pub const PICTURE_HEIGHT: u32 = 360;
/// Rows of incompressible noise that bring the PNG to about 70 KB.
const NOISE_ROWS: u32 = 37;

/// One OBS-sized picture, made once. Every picture URL serves it; the page asks with a new
/// query each time and the server answers `no-store`, so each load crosses the network.
pub fn picture() -> &'static [u8] {
    static PICTURE: OnceLock<Vec<u8>> = OnceLock::new();
    PICTURE.get_or_init(|| {
        let (w, h) = (PICTURE_WIDTH, PICTURE_HEIGHT);
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        let mut seed: u32 = 0x9e37_79b9;
        for y in 0..h {
            for _ in 0..w {
                if y < NOISE_ROWS {
                    seed ^= seed << 13;
                    seed ^= seed >> 17;
                    seed ^= seed << 5;
                    rgb.extend_from_slice(&seed.to_le_bytes()[..3]);
                } else {
                    // Bands of one colour per row compress to almost nothing.
                    let shade = (y * 255 / h) as u8;
                    rgb.extend_from_slice(&[shade, 120, 160]);
                }
            }
        }
        // An encoder over a buffer in memory cannot fail; an empty picture would fail the
        // image probes visibly rather than hide the cause.
        crate::png::encode_rgb(w, h, &rgb).unwrap_or_default()
    })
}

/// The aligner view of one verse as JSON, at least `bytes` long: a word bank and cards, the
/// shape of the reply the real `align/edit` sends (AD18).
pub fn aligner_view(bytes: usize) -> &'static str {
    static VIEW: OnceLock<String> = OnceLock::new();
    VIEW.get_or_init(|| {
        let mut bank = Vec::new();
        let mut cards = Vec::new();
        let mut i = 0usize;
        loop {
            bank.push(serde_json::json!({
                "id": format!("b{i}"),
                "text": format!("word{i}"),
                "occurrence": 1 + i % 2,
                "occurrences": 2,
                "armed": false,
                "suggested": i.is_multiple_of(7),
            }));
            cards.push(serde_json::json!({
                "id": format!("c{i}"),
                "source": [{
                    "text": "וְ\u{5b0}אָהַבְתָּ",
                    "lemma": "אָהַב",
                    "strong": format!("H{:04}", 157 + i),
                    "morph": "He,C:Vqq2ms",
                    "occurrence": 1,
                    "occurrences": 1,
                }],
                "target": [{ "id": format!("t{i}"), "text": format!("love{i}") }],
            }));
            let view = serde_json::json!({
                "verse": "PSA 119:176",
                "bank": bank,
                "cards": cards,
            })
            .to_string();
            if view.len() >= bytes {
                return view;
            }
            i += 1;
        }
    })
}

/// The reply to one click: a sequence number, the armed word, and the verse's view.
pub fn click_reply(seq: u64, word: u64, bytes: usize) -> String {
    format!(
        "{{\"seq\":{seq},\"armed\":\"b{word}\",\"view\":{}}}",
        aligner_view(bytes)
    )
}

/// A download's body: a few kilobytes of text.
pub fn download_body(id: &str) -> String {
    let mut body = String::new();
    for line in 0..64 {
        body.push_str(&format!("tc4 spike download {id}, line {line}\n"));
    }
    body
}
