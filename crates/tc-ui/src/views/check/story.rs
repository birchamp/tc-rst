//! A story check's source text (unit B7; tC4 `StoryPane`): the gateway story's frame, with the
//! quote's words lit. The rest of a story check's detail is a book's (`super::detail`, C4).

use leptos::prelude::*;
use tc_api::check::StoryPane;

use crate::ds::Overline;
use crate::i18n::{keys, t};

const SCRIPTURE: &str = "var(--font-scripture)";

/// The compare card's gateway story card (tC4 `StoryPane`, `sourceCardStyle` since #762).
pub(super) fn story_pane(pane: &StoryPane) -> AnyView {
    let body = match pane {
        StoryPane::Text { tokens, lit } => {
            let text = crate::css! {
                "text-align": "start",
                "font-family": SCRIPTURE,
                "font-size": "var(--fs-verse)",
                "line-height": "var(--lh-verse-md)",
                "color": "var(--text-scripture)",
                "white-space": "pre-wrap",
                "margin": "8px 0 0",
                "max-width": "80ch",
            };
            let mark = crate::css! {
                "background": "var(--tc-highlight-soft)",
                "color": "var(--text-heading)",
                "border-radius": "var(--radius-xs)",
                "padding": "0 .06em",
            };
            let parts = tokens
                .iter()
                .enumerate()
                .map(|(at, token)| {
                    let token = token.clone();
                    let on = u32::try_from(at).is_ok_and(|at| lit.contains(&at));
                    if on {
                        view! { <mark data-testid="story-hl" style=mark.to_style()>{token}</mark> }
                            .into_any()
                    } else {
                        token.into_any()
                    }
                })
                .collect_view();
            view! { <p data-testid="story-pane-text" dir="auto" style=text.to_style()>{parts}</p> }
                .into_any()
        }
        StoryPane::Missing => {
            let style = crate::css! {
                "font-size": "var(--fs-ui-sm)",
                "color": "var(--uw-haze)",
                "font-style": "italic",
                "margin": "8px 0 0",
            };
            view! { <p style=style.to_style()>{t(keys::story_draft::SOURCE_MISSING, &[])}</p> }
                .into_any()
        }
    };
    view! {
        <div data-testid="story-pane" style=super::detail::source_card().to_style()>
            <Overline tone=crate::ds::OverlineTone::Muted>{t(keys::check::GATEWAY_STORY_LABEL, &[])}</Overline>
            {body}
        </div>
    }
    .into_any()
}
