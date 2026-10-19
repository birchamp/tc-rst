//! The port of tC4's `SourceVerse.jsx` (unit B3): one source verse, its words lit where the
//! focused help's quote is rendered. The server finds the words (tc-helps' highlight); the
//! verse shows them. With no focus on the verse, it is its text.

use leptos::prelude::*;
use tc_api::editor::PaneVerse;

use crate::app::understand::HelpFocus;

/// The verse `verse` of pane `pane` under `focus`.
pub fn source_verse(verse: &PaneVerse, pane: &str, focus: Option<&HelpFocus>) -> AnyView {
    let lit = focus
        .map(|f| f.lit(pane, verse.key.as_str()))
        .unwrap_or_default();
    if lit.is_empty() {
        return verse.text.clone().into_any();
    }
    let mark = crate::css! {
        "background": "var(--tc-highlight-soft)",
        "color": "var(--text-heading)",
        "border-radius": "var(--radius-xs)",
        "padding": "0 .06em",
    }
    .to_style();
    verse
        .tokens
        .iter()
        .enumerate()
        .map(|(i, token)| {
            if lit.contains(&i) {
                view! { <mark data-testid="source-hl" style=mark.clone()>{token.clone()}</mark> }
                    .into_any()
            } else {
                token.clone().into_any()
            }
        })
        .collect_view()
        .into_any()
}
