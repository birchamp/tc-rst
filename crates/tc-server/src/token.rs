//! The launch's token and the line that announces it (APP-DESIGN §5.1, §5.2).

use std::fmt;

use base64::Engine as _;
use serde::Serialize;
use subtle::ConstantTimeEq as _;

use crate::ServerError;

/// 32 random bytes, base64url without padding: 43 characters. Every request must carry it.
pub struct LaunchToken(String);

impl LaunchToken {
    /// A failure of the operating system's random source is an error, never a weaker token.
    pub fn new() -> Result<Self, ServerError> {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| ServerError::Random)?;
        Ok(Self(
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes),
        ))
    }

    /// The only way the token leaves this type: for the address line and the debug redirect.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Compares in time that does not depend on how many leading bytes agree.
    pub(crate) fn matches(&self, value: &[u8]) -> bool {
        bool::from(self.0.as_bytes().ct_eq(value))
    }
}

impl fmt::Debug for LaunchToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LaunchToken(..)")
    }
}

/// The one line `tc4 serve` prints before anything else: where the server is and the token to
/// send. The token appears nowhere else.
#[derive(Serialize)]
pub struct AddressLine {
    pub url: String,
    pub token: String,
}

impl fmt::Debug for AddressLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AddressLine")
            .field("url", &self.url)
            .field("token", &"..")
            .finish()
    }
}

impl AddressLine {
    /// The JSON text of the line, without its newline.
    pub fn to_json(&self) -> String {
        // Two strings: serializing them cannot fail.
        serde_json::to_string(self).unwrap_or_default()
    }
}
