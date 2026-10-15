//! Secrets: the password a person types once, and the token Door43 mints from it.
//!
//! Decision D85: the token lives in memory or in the OS keychain, nowhere else. These types
//! have no serde impls and no `Display`, and their `Debug` prints a placeholder, so a
//! secret cannot reach a log line, a settings file or an error message by accident. The
//! client sends a secret only in the `Authorization` header, marked sensitive, and never
//! in a URL.

use crate::names::{AccountName, ParseError};
use base64::Engine;
use reqwest::header::HeaderValue;
use std::fmt;

const REDACTED: &str = "<redacted>";

/// The header value for `text`, marked sensitive so the HTTP stack never logs it. Every
/// caller has checked `text`, so a refusal here is the caller's refusal too.
fn sensitive_header(text: &str, refusal: ParseError) -> Result<HeaderValue, ParseError> {
    let mut header = HeaderValue::from_str(text).map_err(|_| refusal)?;
    header.set_sensitive(true);
    Ok(header)
}

/// Door43 tokens are 40 hex digits; the rule here is wider (any visible ASCII), and only
/// refuses what an HTTP header cannot carry or would change, such as a trailing newline.
fn is_sendable_secret(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_graphic())
}

/// A Door43 access token (the `sha1` field of the create-token answer).
#[derive(Clone)]
pub struct Token {
    secret: String,
    /// `token <secret>`, built once so a token that exists can always be sent.
    header: HeaderValue,
}

impl Token {
    /// Wrap a token read back from the OS keychain. A token that is empty or holds anything
    /// but visible ASCII (a stray newline, a space) is refused here, so it is never
    /// mistaken for one Door43 revoked.
    pub fn new(secret: String) -> Result<Self, ParseError> {
        let refusal = || ParseError::InvalidSecret("token");
        if !is_sendable_secret(&secret) {
            return Err(refusal());
        }
        let header = sensitive_header(&format!("token {secret}"), refusal())?;
        Ok(Self { secret, header })
    }

    /// The secret itself, for the OS keychain and the git credential callback only.
    pub fn expose_secret(&self) -> &str {
        &self.secret
    }

    pub(crate) fn header(&self) -> &HeaderValue {
        &self.header
    }
}

impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        self.secret == other.secret
    }
}

impl Eq for Token {}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Token({REDACTED})")
    }
}

/// A password, held only for the length of a sign-in. Any text is allowed: it travels
/// base64-encoded inside the `Authorization` header.
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
#[derive(Clone)]
pub struct OneTimeCode {
    code: String,
    header: HeaderValue,
}

impl OneTimeCode {
    /// Refuses a code that is empty or holds anything but visible ASCII.
    pub fn new(code: String) -> Result<Self, ParseError> {
        let refusal = || ParseError::InvalidSecret("one-time code");
        if !is_sendable_secret(&code) {
            return Err(refusal());
        }
        let header = sensitive_header(&code, refusal())?;
        Ok(Self { code, header })
    }

    pub(crate) fn expose_secret(&self) -> &str {
        &self.code
    }

    pub(crate) fn header(&self) -> &HeaderValue {
        &self.header
    }
}

impl PartialEq for OneTimeCode {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code
    }
}

impl Eq for OneTimeCode {}

impl fmt::Debug for OneTimeCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OneTimeCode({REDACTED})")
    }
}

/// What a person types to sign in: a username or an email, and the password.
#[derive(Clone)]
pub struct Credentials {
    login: String,
    password: Password,
    one_time_code: Option<OneTimeCode>,
    /// `base64(login:password)`, encoded from UTF-8 bytes so a password outside Latin-1
    /// still works.
    basic: String,
    basic_header: HeaderValue,
}

impl Credentials {
    /// `login` is a username or an email. Basic auth cannot carry a `:` in it.
    pub fn new(login: &str, password: Password) -> Result<Self, ParseError> {
        if login.is_empty() || login.contains(':') || login.chars().any(char::is_control) {
            return Err(ParseError::InvalidLogin);
        }
        let basic = base64::engine::general_purpose::STANDARD
            .encode(format!("{login}:{}", password.expose_secret()));
        // Base64 text is always a valid header value.
        let basic_header = sensitive_header(&format!("Basic {basic}"), ParseError::InvalidLogin)?;
        Ok(Self {
            login: login.to_owned(),
            password,
            one_time_code: None,
            basic,
            basic_header,
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

    pub(crate) fn basic(&self) -> &str {
        &self.basic
    }

    pub(crate) fn basic_header(&self) -> &HeaderValue {
        &self.basic_header
    }
}

impl fmt::Debug for Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credentials")
            .field("login", &self.login)
            .field("password", &self.password)
            .field("one_time_code", &self.one_time_code)
            .finish_non_exhaustive()
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
            token: Token::new("0123456789abcdef-token".into()).unwrap(),
        };
        let credentials = Credentials::new("translator", Password::new("hunter2-pass".into()))
            .unwrap()
            .with_one_time_code(OneTimeCode::new("424242".into()).unwrap());
        let shown = format!("{session:?} {session:#?} {credentials:?} {credentials:#?}");
        let basic = credentials.basic().to_owned();
        for secret in ["0123456789abcdef-token", "hunter2-pass", "424242", &basic] {
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

    #[test]
    fn a_secret_that_cannot_travel_in_a_header_is_refused_when_made() {
        for bad in ["", "abc\n", "abc\r\n", "a b", "tab\t", "ünïcode", "\u{7f}"] {
            let token = Token::new(bad.to_owned());
            assert_eq!(token, Err(ParseError::InvalidSecret("token")), "{bad:?}");
            assert!(OneTimeCode::new(bad.to_owned()).is_err(), "{bad:?}");
        }
        let refused = Token::new("hunter2\n".to_owned()).unwrap_err().to_string();
        assert!(!refused.contains("hunter2"), "{refused}");
        let token = Token::new("0123456789abcdef0123456789abcdef01234567".into()).unwrap();
        assert_eq!(
            token.header().to_str().unwrap(),
            "token 0123456789abcdef0123456789abcdef01234567"
        );
        assert!(token.header().is_sensitive());
    }

    #[test]
    fn any_password_can_be_sent() {
        let credentials =
            Credentials::new("translator", Password::new("pässwörd ✓\n\t \"".into())).unwrap();
        assert!(credentials.basic_header().is_sensitive());
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(credentials.basic())
            .unwrap();
        assert_eq!(decoded, "translator:pässwörd ✓\n\t \"".as_bytes());
    }
}
