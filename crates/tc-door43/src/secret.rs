//! Secrets: the password a person types once, and the token Door43 mints from it.
//!
//! Decision D85: the token lives in memory or in the OS keychain, nowhere else. These types
//! have no serde impls and no `Display`, and their `Debug` prints a placeholder, so a
//! secret cannot reach a log line, a settings file or an error message by accident. The
//! client sends a secret only in the `Authorization` header, marked sensitive, and never
//! in a URL.

use crate::names::{AccountName, ParseError};
use std::fmt;

const REDACTED: &str = "<redacted>";

/// A Door43 access token (the `sha1` field of the create-token answer).
#[derive(Clone, PartialEq, Eq)]
pub struct Token(String);

impl Token {
    /// Wrap a token read back from the OS keychain.
    pub fn new(secret: String) -> Self {
        Self(secret)
    }

    /// The secret itself, for the OS keychain and the git credential callback only.
    pub fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Token({REDACTED})")
    }
}

/// A password, held only for the length of a sign-in.
#[derive(Clone, PartialEq, Eq)]
pub struct Password(String);

impl Password {
    pub fn new(secret: String) -> Self {
        Self(secret)
    }

    pub(crate) fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Password({REDACTED})")
    }
}

/// A one-time code for an account with two-factor sign-in (`X-GITEA-OTP`).
#[derive(Clone, PartialEq, Eq)]
pub struct OneTimeCode(String);

impl OneTimeCode {
    pub fn new(code: String) -> Self {
        Self(code)
    }

    pub(crate) fn expose_secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for OneTimeCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OneTimeCode({REDACTED})")
    }
}

/// What a person types to sign in: a username or an email, and the password.
#[derive(Clone, Debug)]
pub struct Credentials {
    login: String,
    password: Password,
    one_time_code: Option<OneTimeCode>,
}

impl Credentials {
    /// `login` is a username or an email. Basic auth cannot carry a `:` in it.
    pub fn new(login: &str, password: Password) -> Result<Self, ParseError> {
        if login.is_empty() || login.contains(':') || login.chars().any(char::is_control) {
            return Err(ParseError::InvalidLogin);
        }
        Ok(Self {
            login: login.to_owned(),
            password,
            one_time_code: None,
        })
    }

    pub fn with_one_time_code(mut self, code: OneTimeCode) -> Self {
        self.one_time_code = Some(code);
        self
    }

    pub fn login(&self) -> &str {
        &self.login
    }

    pub(crate) fn password(&self) -> &Password {
        &self.password
    }

    pub(crate) fn one_time_code(&self) -> Option<&OneTimeCode> {
        self.one_time_code.as_ref()
    }
}

/// A signed-in Door43 account: the account login and its token. The login comes from
/// Door43's own answer, never from what was typed, because an email cannot be used in a
/// route.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Session {
    pub username: AccountName,
    pub token: Token,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_a_secret() {
        let session = Session {
            username: "translator".parse().unwrap(),
            token: Token::new("0123456789abcdef-token".into()),
        };
        let credentials = Credentials::new("translator", Password::new("hunter2-pass".into()))
            .unwrap()
            .with_one_time_code(OneTimeCode::new("424242".into()));
        let shown = format!("{session:?} {session:#?} {credentials:?} {credentials:#?}");
        for secret in ["0123456789abcdef-token", "hunter2-pass", "424242"] {
            assert!(!shown.contains(secret), "{shown}");
        }
        assert!(shown.contains("translator"));
    }

    #[test]
    fn a_login_that_breaks_basic_auth_is_refused() {
        let password = || Password::new("p".into());
        assert!(Credentials::new("", password()).is_err());
        assert!(Credentials::new("a:b", password()).is_err());
        assert!(Credentials::new("a\nb", password()).is_err());
        assert!(Credentials::new("translator@example.org", password()).is_ok());
    }
}
