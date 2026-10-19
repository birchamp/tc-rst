//! The Door43 account (APP-DESIGN §5.6) and the `account` service. The Door43 token never
//! reaches the UI: the server keeps it in memory, and in the keychain with "Stay signed in".

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::{IdError, RequestId};

/// What the account menu and the sign-in step show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    /// The signed-in Door43 username, or `None`.
    pub user: Option<String>,
    pub kept: KeepState,
    /// What the last account step could not do, in the account menu's words.
    pub notice: Option<AccountNotice>,
}

/// Whether a sign-in is kept on this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum KeepState {
    /// The sign-in lasts for this app session.
    NotKept,
    /// The keychain holds a token: "Sign-in saved on this computer". It is checked with
    /// Door43 only when a step needs it.
    Kept,
    /// "Stay signed in" was asked, and this computer has no keychain that would keep it
    /// (`signIn.notKept`).
    NoKeychain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum AccountNotice {
    /// `account.signOutFailed`: signed out for this session, but the keychain kept the token.
    SignOutFailed,
    /// `account.savedRefused`: Door43 refused the kept token, so it was removed.
    SavedRefused,
    /// `account.savedUnchecked`: Door43 did not answer; the kept token stays.
    SavedUnchecked,
}

impl AccountNotice {
    pub fn key(self) -> &'static str {
        match self {
            Self::SignOutFailed => "account.signOutFailed",
            Self::SavedRefused => "account.savedRefused",
            Self::SavedUnchecked => "account.savedUnchecked",
        }
    }
}

/// `account/state`: reads whether a token is kept. Sends nothing to Door43.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct State {}

/// `account/signIn`: the login and password go to Door43 once, to mint a token, and are
/// not kept. A result that arrives after `account/cancelSignIn` is dropped.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignIn {
    pub request: RequestId,
    pub login: String,
    pub password: String,
    /// "Stay signed in on this computer".
    pub stay: bool,
}

impl fmt::Debug for SignIn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignIn")
            .field("request", &self.request)
            .field("login", &self.login)
            .field("password", &"..")
            .field("stay", &self.stay)
            .finish()
    }
}

/// `account/cancelSignIn`: the sign-in in flight ends; a token that arrives after it is
/// forgotten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelSignIn {
    pub request: RequestId,
}

/// `account/resume`: asks Door43 who the kept token belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Resume {
    pub request: RequestId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct ResumeView {
    pub outcome: ResumeOutcome,
    pub account: AccountView,
}

/// What a resume found (`tC4:src/data/share/session.ts`, `ResumeOutcome`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub enum ResumeOutcome {
    Resumed,
    /// No token is kept.
    NoneKept,
    /// Door43 answered 401 or 403; the token was forgotten.
    Refused,
    /// Door43 did not answer; the token stays kept.
    Unavailable,
}

/// `account/signOut`: forgets the token in memory and in the keychain. Sends nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SignOut {
    pub request: RequestId,
}

/// `account/profileLink`: the address of the signed-in person's Door43 page, which the
/// shell opens in the system browser.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileLink {}

/// `account/helpLink`: a link in a help page, which the shell opens in the system browser
/// once the internet question was answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HelpLink {
    pub url: WebUrl,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "proptest", derive(proptest_derive::Arbitrary))]
#[serde(rename_all = "camelCase")]
pub struct LinkView {
    pub url: WebUrl,
}

/// An `http` or `https` address, the only kind the shell opens outside the app.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct WebUrl(String);

impl WebUrl {
    pub const MAX_BYTES: usize = 8192;

    pub fn parse(text: &str) -> Result<Self, IdError> {
        let refuse = |reason| {
            Err(IdError {
                what: "web address",
                reason,
            })
        };
        let rest = text
            .strip_prefix("https://")
            .or_else(|| text.strip_prefix("http://"));
        let Some(rest) = rest else {
            return refuse("not an http or https address");
        };
        if rest.is_empty() || rest.starts_with('/') {
            return refuse("no host");
        }
        if text.len() > Self::MAX_BYTES {
            return refuse("too long");
        }
        if text.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return refuse("holds a space or a control character");
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for WebUrl {
    type Error = IdError;
    fn try_from(text: String) -> Result<Self, IdError> {
        Self::parse(&text)
    }
}

impl From<WebUrl> for String {
    fn from(url: WebUrl) -> Self {
        url.0
    }
}

#[cfg(feature = "proptest")]
impl proptest::arbitrary::Arbitrary for WebUrl {
    type Parameters = ();
    type Strategy = proptest::strategy::BoxedStrategy<Self>;
    fn arbitrary_with((): ()) -> Self::Strategy {
        use proptest::strategy::Strategy;
        // The pattern is a literal that compiles.
        proptest::string::string_regex("https?://[a-z0-9.-]{1,20}(/[A-Za-z0-9._~%-]{0,12}){0,3}")
            .unwrap_or_else(|_| unreachable!("a valid regex"))
            .prop_map(WebUrl)
            .boxed()
    }
}
