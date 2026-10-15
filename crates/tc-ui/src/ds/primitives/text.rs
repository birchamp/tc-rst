//! Text.jsx: a run of text in one of the system's type roles.

use leptos::prelude::*;

use crate::ds::{
    Attrs, Css, Measure, MeasureToken, Script, Tag, TextRole, TextTone, element, spread,
};

/// The role's declarations, in Text.jsx's key order, and whether it trims to cap height.
fn role_css(role: TextRole) -> (Css, bool) {
    use TextRole as R;
    let c = match role {
        R::Display => {
            css! { "font-size": "var(--fs-display)", "font-weight": "var(--fw-black)", "line-height": "var(--lh-tight)", "letter-spacing": "var(--track-38)", "color": "var(--fg-strong)" }
        }
        R::H1 => {
            css! { "font-size": "var(--fs-h1)", "font-weight": "var(--fw-black)", "line-height": "var(--lh-tight)", "letter-spacing": "var(--track-32)", "color": "var(--fg-strong)" }
        }
        R::H2 => {
            css! { "font-size": "var(--fs-h2)", "font-weight": "var(--fw-black)", "line-height": "var(--lh-tight)", "letter-spacing": "var(--track-22)", "color": "var(--fg-strong)" }
        }
        R::H3 => {
            css! { "font-size": "var(--fs-h3)", "font-weight": "var(--fw-black)", "line-height": "var(--lh-tight)", "letter-spacing": "var(--track-20)", "color": "var(--fg-strong)" }
        }
        R::Title => {
            css! { "font-size": "var(--fs-title-lg)", "font-weight": "var(--fw-heavy)", "line-height": "var(--lh-tight)", "letter-spacing": "var(--track-19)", "color": "var(--fg-strong)" }
        }
        R::TitleSm => {
            css! { "font-size": "var(--fs-ui-lg)", "font-weight": "var(--fw-heavy)", "line-height": "var(--lh-tight)", "letter-spacing": "var(--track-14-5)", "color": "var(--fg-strong)" }
        }
        R::Strong => {
            css! { "font-size": "var(--fs-ui-md)", "font-weight": "var(--fw-heavy)", "line-height": "var(--lh-ui)", "letter-spacing": "var(--track-14)", "color": "var(--fg-strong)" }
        }
        R::Body => {
            css! { "font-size": "var(--fs-ui-md)", "font-weight": "var(--fw-regular)", "line-height": "var(--lh-body)", "letter-spacing": "var(--track-14)", "color": "var(--fg)" }
        }
        R::Ui => {
            css! { "font-size": "var(--fs-ui-sm)", "font-weight": "var(--fw-bold)", "line-height": "var(--lh-ui)", "letter-spacing": "var(--track-13)", "color": "var(--fg)" }
        }
        R::Caption => {
            css! { "font-size": "var(--fs-caption-lg)", "font-weight": "var(--fw-regular)", "line-height": "var(--lh-ui)", "letter-spacing": "var(--track-12-5)", "color": "var(--fg-muted)" }
        }
        R::CaptionStrong => {
            css! { "font-size": "var(--fs-caption)", "font-weight": "var(--fw-bold)", "line-height": "var(--lh-ui)", "letter-spacing": "var(--track-12)", "color": "var(--fg-muted)" }
        }
        R::Overline => {
            css! { "font-size": "var(--fs-label)", "font-weight": "var(--fw-heavy)", "letter-spacing": "var(--tracking-overline)", "text-transform": "uppercase", "line-height": "var(--lh-ui)", "color": "var(--fg-muted)" }
        }
        R::Label => {
            css! { "font-size": "var(--fs-badge)", "font-weight": "var(--fw-heavy)", "letter-spacing": "var(--tracking-label)", "text-transform": "uppercase", "line-height": "var(--lh-ui)", "color": "var(--fg-muted)" }
        }
        R::LabelNum => {
            css! { "font-size": "var(--fs-badge)", "font-weight": "var(--fw-heavy)", "letter-spacing": 0, "font-variant-numeric": "tabular-nums", "line-height": "var(--lh-ui)", "color": "var(--fg-muted)" }
        }
        R::LabelMicro => {
            css! { "font-size": "var(--fs-micro)", "font-weight": "var(--fw-heavy)", "letter-spacing": "var(--tracking-label)", "text-transform": "uppercase", "line-height": "var(--lh-ui)", "color": "var(--fg-muted)" }
        }
        R::Meta => {
            css! { "font-size": "var(--fs-meta)", "font-family": "var(--font-mono)", "font-weight": "var(--fw-regular)", "line-height": "var(--lh-ui)", "letter-spacing": "var(--track-11-5)", "color": "var(--fg-muted)" }
        }
        R::Scripture => {
            css! { "font-family": "var(--font-scripture)", "font-size": "var(--fs-verse-lg)", "line-height": "var(--lh-verse-lg)", "color": "var(--text-scripture)" }
        }
        R::ScriptureMd => {
            css! { "font-family": "var(--font-scripture)", "font-size": "var(--fs-verse-md)", "line-height": "var(--lh-verse-md)", "color": "var(--text-scripture)" }
        }
        R::ScriptureSm => {
            css! { "font-family": "var(--font-scripture)", "font-size": "var(--fs-verse-sm)", "line-height": "var(--lh-verse-sm)", "color": "var(--text-scripture)" }
        }
        R::Quote => {
            css! { "font-family": "var(--font-scripture)", "font-size": "var(--fs-verse-sm)", "font-weight": "var(--fw-bold)", "line-height": "var(--lh-verse-sm)", "color": "var(--fg-strong)" }
        }
    };
    let trim = matches!(
        role,
        R::Display | R::H1 | R::H2 | R::H3 | R::Title | R::TitleSm
    );
    (c, trim)
}

fn tone_color(tone: TextTone) -> Option<&'static str> {
    match tone {
        TextTone::Default => None,
        TextTone::Strong => Some("var(--fg-strong)"),
        TextTone::Muted => Some("var(--fg-muted)"),
        TextTone::Faint => Some("var(--fg-faint)"),
        TextTone::Accent => Some("var(--text-accent)"),
        TextTone::Tone => Some("var(--tone-text)"),
        TextTone::Inverse => Some("#fff"),
        TextTone::Scripture => Some("var(--text-scripture)"),
    }
}

/// Charis covers no Greek, Hebrew or Arabic: the other faces carry `font-size-adjust` to read
/// at Charis's optical size, except Nastaliq, which keeps its own size and leading.
fn script_css(script: Script) -> Css {
    match script {
        Script::Latin => css! { "font-family": "var(--font-scripture)" },
        Script::Greek => {
            css! { "font-family": "var(--font-greek)", "font-size-adjust": "var(--fs-adjust-scripture)" }
        }
        Script::Hebrew => {
            css! { "font-family": "var(--font-hebrew)", "font-size-adjust": "var(--fs-adjust-scripture)" }
        }
        Script::Arabic => {
            css! { "font-family": "var(--font-arabic)", "font-size-adjust": "var(--fs-adjust-scripture)" }
        }
        Script::Nastaliq => {
            css! { "font-family": "var(--font-nastaliq)", "font-size": "var(--fs-verse-nastaliq)", "line-height": "var(--lh-nastaliq)" }
        }
    }
}

/// The inline style Text.jsx writes.
pub(crate) fn text_css(
    role: TextRole,
    tone: TextTone,
    script: Option<Script>,
    align: Option<&str>,
    measure: Option<&Measure>,
    truncate: bool,
    style: &Css,
) -> (Css, bool) {
    let (role_style, trim) = role_css(role);
    let mut s = css! { "font-family": "var(--font-ui)", "margin": 0 };
    s.extend(&role_style);
    if let Some(sc) = script {
        s.extend(&script_css(sc));
    }
    let role_color = role_style.get("color");
    s.set_opt("color", tone_color(tone).map(String::from).or(role_color));
    s.set_opt("text-align", align.map(String::from));
    if let Some(m) = measure {
        let w = match m {
            Measure::Token(MeasureToken::Read) => "var(--measure-read)".to_string(),
            Measure::Token(MeasureToken::Prose) => "var(--measure-prose)".to_string(),
            Measure::Token(MeasureToken::Narrow) => "var(--measure-narrow)".to_string(),
            Measure::Css(c) => c.clone(),
        };
        s.set("max-width", w);
    }
    if truncate {
        s.extend(&css! { "white-space": "nowrap", "overflow": "hidden", "text-overflow": "ellipsis", "min-width": 0 });
    }
    s.extend(style);
    (s, trim)
}

/// A run of text in one of the system's type roles. `tag` defaults to `p` for `body` and
/// `caption`, `span` otherwise; `trim` overrides the role's cap-height trim.
#[component]
pub fn Text(
    #[prop(optional)] role: TextRole,
    #[prop(optional, into)] tag: Option<Tag>,
    #[prop(optional)] tone: TextTone,
    #[prop(optional)] script: Option<Script>,
    #[prop(optional, into)] align: Option<String>,
    #[prop(optional)] truncate: bool,
    #[prop(optional)] trim: Option<bool>,
    #[prop(optional)] measure: Option<Measure>,
    #[prop(optional, into)] dir: Option<String>,
    /// May follow state (an Accordion's chevron turns), on the same node.
    #[prop(optional, into)]
    style: Signal<Css>,
    #[prop(optional)] attrs: Attrs,
    #[prop(optional)] children: Option<Children>,
) -> impl IntoView {
    let tag = tag.unwrap_or_else(|| {
        if matches!(role, TextRole::Body | TextRole::Caption) {
            "p".into()
        } else {
            "span".into()
        }
    });
    let cap = trim.unwrap_or(role_css(role).1);
    let dir = dir.or_else(|| script.filter(|s| s.is_rtl()).map(|_| "rtl".to_string()));
    let s = move || {
        style.with(|st| {
            let (s, _) = text_css(
                role,
                tone,
                script,
                align.as_deref(),
                measure.as_ref(),
                truncate,
                st,
            );
            s.to_style()
        })
    };
    element(tag)
        .attr("data-trim", cap.then_some("cap"))
        .attr("dir", dir)
        .attr("style", s)
        .add_any_attr(spread(&attrs))
        .child(children.map(|c| c()))
}
