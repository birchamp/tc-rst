//! The shell's decisions (tC4's `App.jsx`, unit A5): which view and project the page shows,
//! what the top bar and the account button say, which screen the main area draws, and where a
//! reload in the same launch returns to (APP-DESIGN 6.6, 6.7). Pure, so they are tested on the
//! host; `web::shell` draws them.

use serde::{Deserialize, Serialize};
use tc_api::account::{AccountView, KeepState};
use tc_api::app::{HelloView, SessionRole};
use tc_api::buffer::BufferKey;
use tc_api::ids::{ProjectId, ProjectRef, Scope, ServerInstance};
use tc_api::shown::Direction;

use crate::i18n::{keys, t};

/// `sessionStorage` key of the navigation snapshot (6.6). "Back to Home" in `boot.js`'s static
/// callout removes it.
pub const NAV_KEY: &str = "tc4.nav.v1";

/// tC4's `s.view` (`tC4:src/state.jsx:372`), by its string values. `Read` is Understand and
/// `Draft` is Translate; `Publish` is Community Checking, which tC4 reaches from Check (D63).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum View {
    Home,
    Read,
    Draft,
    Check,
    Publish,
}

impl View {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Read => "read",
            Self::Draft => "draft",
            Self::Check => "check",
            Self::Publish => "publish",
        }
    }

    pub fn parse(s: &str) -> Option<View> {
        [
            Self::Home,
            Self::Read,
            Self::Draft,
            Self::Check,
            Self::Publish,
        ]
        .into_iter()
        .find(|view| view.as_str() == s)
    }
}

/// tC4's `project.flavor` (`tC4:src/App.jsx:194`): a Bible or a set of stories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProjectFlavor {
    TextTranslation,
    TextStories,
}

/// The project the screens show: the fields of tC4's `s.project` the shell reads. B3's open
/// fills it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentProject {
    pub at: Scope,
    pub id: ProjectId,
    pub name: String,
    pub language_tag: String,
    /// The direction of the project's own script (`projectDir`, `tC4:src/views/scriptStyle.js:31`).
    pub dir: Direction,
    pub flavor: ProjectFlavor,
}

/// The editing lock (6.5): an open editing card keeps the exits off, because leaving would
/// close the card that holds its draft (tC4 #238, #698).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Editing {
    #[default]
    None,
    Verse(BufferKey),
    Section(BufferKey),
    Frame(BufferKey),
}

impl Editing {
    pub fn is_open(&self) -> bool {
        !matches!(self, Self::None)
    }
}

/// JavaScript's `\s`: what tC4's `split(/\s+/)` and `trim()` treat as space. Rust's
/// `char::is_whitespace` adds U+0085 and leaves out U+FEFF.
pub fn js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// JavaScript's `trim()`.
pub fn js_trim(text: &str) -> &str {
    text.trim_matches(js_space)
}

/// The project chip's initials (`tC4:src/App.jsx:99`): `(name || '?').split(/\s+/).map((w) =>
/// w[0]).slice(0, 2).join('').toUpperCase()`. A name of spaces gives nothing, and a name that
/// starts with a space gives one letter, as in tC4. Declared difference: a word that starts
/// outside the Basic Multilingual Plane gives its whole first character, where tC4's `w[0]`
/// gives half a surrogate pair, a broken glyph.
pub fn project_initials(name: &str) -> String {
    let name = if name.is_empty() { "?" } else { name };
    let initials: String = js_split_space(name)
        .into_iter()
        .take(2)
        .filter_map(|word| word.chars().next())
        .collect();
    initials.to_uppercase()
}

/// JavaScript's `split(/\s+/)`: the words between runs of space, with an empty word before a
/// leading run and after a trailing one.
fn js_split_space(text: &str) -> Vec<&str> {
    let mut words = Vec::new();
    let (mut start, mut in_space) = (0, false);
    for (at, c) in text.char_indices() {
        match (js_space(c), in_space) {
            (true, false) => {
                words.push(text.get(start..at).unwrap_or_default());
                in_space = true;
            }
            (false, true) => {
                start = at;
                in_space = false;
            }
            _ => {}
        }
    }
    words.push(if in_space {
        ""
    } else {
        text.get(start..).unwrap_or_default()
    });
    words
}

/// The signed-in account's initials on the account button (`tC4:AccountMenu.jsx:65`):
/// `user.slice(0, 2).toUpperCase()`. Door43 usernames are ASCII.
pub fn user_initials(user: &str) -> String {
    user.chars().take(2).collect::<String>().to_uppercase()
}

/// What the account button knows (`tC4:AccountMenu.jsx:83-85`): signed in, a sign-in kept on
/// this computer, or neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountState<'a> {
    In(&'a str),
    Saved,
    Out,
}

pub fn account_state(view: &AccountView) -> AccountState<'_> {
    match (&view.user, view.kept) {
        (Some(user), _) => AccountState::In(user),
        (None, KeepState::Kept) => AccountState::Saved,
        (None, _) => AccountState::Out,
    }
}

impl AccountState<'_> {
    /// The button's `data-state`.
    pub fn data_state(&self) -> &'static str {
        match self {
            Self::In(_) => "in",
            Self::Saved => "saved",
            Self::Out => "out",
        }
    }

    /// `t('account.label.{state}', { user })`, in the shown language.
    pub fn label(&self) -> String {
        match self {
            Self::In(user) => t(keys::account::label::IN, &[("user", user)]),
            Self::Saved => t(keys::account::label::SAVED, &[]),
            Self::Out => t(keys::account::label::OUT, &[]),
        }
    }
}

/// Whether the page shows a project's screens: tC4's `!!s.project && s.view !== 'home'`
/// (`tC4:src/App.jsx:88`), which draws the mode switch.
pub fn in_project(view: View, project: Option<&CurrentProject>) -> bool {
    project.is_some() && view != View::Home
}

/// The screen tC4's `MainView` draws (`tC4:src/App.jsx:192-209`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Understand,
    Draft,
    Check,
    CommunityChecking,
    StoryUnderstand,
    StoryDraft,
    /// `views[state.view] || null`: a view with no project draws nothing.
    Nothing,
}

pub fn screen(view: View, project: Option<&CurrentProject>) -> Screen {
    if view == View::Home {
        return Screen::Home;
    }
    if project.is_some_and(|p| p.flavor == ProjectFlavor::TextStories) {
        return match view {
            View::Read => Screen::StoryUnderstand,
            View::Check => Screen::Check,
            View::Publish => Screen::CommunityChecking,
            View::Home | View::Draft => Screen::StoryDraft,
        };
    }
    // tC4 draws the Bible views whether or not a project is open; each view then shows its own
    // empty state. Without a project the shell never leaves Home, so these arms need one.
    match (view, project) {
        (_, None) => Screen::Nothing,
        (View::Read, Some(_)) => Screen::Understand,
        (View::Draft, Some(_)) => Screen::Draft,
        (View::Check, Some(_)) => Screen::Check,
        (View::Publish, Some(_)) => Screen::CommunityChecking,
        (View::Home, Some(_)) => Screen::Home,
    }
}

/// `sessionStorage[NAV_KEY]` (6.6): where the page was, in which server run. Screen units add
/// their own fields (a book, a chapter, a tool) with `#[serde(default)]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NavSnapshot {
    pub instance: ServerInstance,
    pub view: View,
    pub project: Option<CurrentProject>,
    /// The open book and chapter (B3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<BookPlace>,
}

/// The book and chapter a project's screens show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookPlace {
    pub book: tc_core::BookId,
    pub chapter: u16,
}

/// The book and chapter a reload returns to, read from the snapshot [`restore`] read; the
/// caller asks only when [`restore`] gave back the project.
pub fn restore_place(kept: Option<&str>) -> Option<BookPlace> {
    serde_json::from_str::<NavSnapshot>(kept?).ok()?.place
}

/// Where a load of the page starts, from the snapshot it kept: the view and the project, only
/// in the same server run and only while `hello` still lists that project as the screen's open
/// session; otherwise Home. A snapshot that cannot be read is Home too.
pub fn restore(kept: Option<&str>, hello: &HelloView) -> (View, Option<CurrentProject>) {
    let home = (View::Home, None);
    let Some(snapshot) = kept.and_then(|text| serde_json::from_str::<NavSnapshot>(text).ok())
    else {
        return home;
    };
    if snapshot.instance != hello.instance {
        return home;
    }
    match snapshot.project {
        None => home,
        Some(project) => {
            let open = hello
                .sessions
                .iter()
                .any(|s| s.at == project.at && s.role == SessionRole::Screen && s.id == project.id);
            if open && snapshot.view != View::Home {
                (snapshot.view, Some(project))
            } else if open {
                (View::Home, None)
            } else {
                home
            }
        }
    }
}

/// The boundary's key (tC4 keys it `${view}:${repoPath}`): a change draws the screen afresh and
/// clears a view error.
pub fn boundary_key(view: View, project: Option<&CurrentProject>) -> (View, Option<ProjectRef>) {
    (view, project.map(|p| p.at.project.clone()))
}
