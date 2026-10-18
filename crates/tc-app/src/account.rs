//! The Door43 account the app holds (APP-DESIGN §4.7, §5.6, Q-3): the signed-in user, the
//! token in memory, and whether the keychain keeps it.
//!
//! The keychain is read once, at start. `account/state` answers from memory and never
//! reaches Door43 or the keychain again, because an operating-system keychain can prompt the
//! person. The sign-in, resume and sign-out steps are unit A8; the OS keychain comes with
//! unit E1a (`keyring`, behind [`Keychain`]).

use std::sync::{Mutex, MutexGuard, PoisonError};

use tc_door43::Token;

/// Where "Stay signed in" keeps a Door43 token.
pub trait Keychain: Send + Sync + 'static {
    /// Keeps the token. `NoKeychain` where this computer has none the app can use
    /// (`signIn.notKept`).
    fn keep(&self, token: &Token) -> Result<(), KeychainError>;

    /// The kept token. Bytes the keychain cannot open are removed and reported as
    /// `Unreadable`, as tC4's `readToken` does: they are no use to anyone.
    fn read(&self) -> Result<Option<Token>, KeychainError>;

    fn forget(&self) -> Result<(), KeychainError>;
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeychainError {
    #[error("this computer has no keychain the app can use")]
    NoKeychain,
    #[error("the kept token could not be read: {0}")]
    Unreadable(String),
    #[error("the keychain refused: {0}")]
    Failed(String),
}

/// How a [`MemKeychain`] behaves, for tests of what the app does when the keychain does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemKeychainMode {
    Working,
    /// This computer has no keychain: nothing is kept, and keeping is refused.
    NoKeychain,
    /// Forgetting fails; keeping and reading work.
    FailingForget,
    /// The kept bytes cannot be opened: the first read removes them and fails.
    Unreadable,
}

/// A keychain in memory. Phase 0 uses it for real, so a sign-in lasts for the app session;
/// tests use it with a mode.
pub struct MemKeychain {
    token: Mutex<Option<Token>>,
    mode: MemKeychainMode,
}

impl Default for MemKeychain {
    fn default() -> Self {
        Self::new()
    }
}

impl MemKeychain {
    pub fn new() -> Self {
        Self::with_mode(MemKeychainMode::Working)
    }

    pub fn holding(token: Token) -> Self {
        Self {
            token: Mutex::new(Some(token)),
            mode: MemKeychainMode::Working,
        }
    }

    pub fn with_mode(mode: MemKeychainMode) -> Self {
        Self {
            token: Mutex::new(None),
            mode,
        }
    }

    /// Puts a token in as if an earlier session had kept it, in whatever mode this is.
    pub fn seed(&self, token: Token) {
        *self.slot() = Some(token);
    }

    fn slot(&self) -> MutexGuard<'_, Option<Token>> {
        self.token.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Keychain for MemKeychain {
    fn keep(&self, token: &Token) -> Result<(), KeychainError> {
        if self.mode == MemKeychainMode::NoKeychain {
            return Err(KeychainError::NoKeychain);
        }
        *self.slot() = Some(token.clone());
        Ok(())
    }

    fn read(&self) -> Result<Option<Token>, KeychainError> {
        let mut slot = self.slot();
        match self.mode {
            MemKeychainMode::NoKeychain => Ok(None),
            MemKeychainMode::Unreadable => {
                if slot.take().is_some() {
                    Err(KeychainError::Unreadable("damaged".to_owned()))
                } else {
                    Ok(None)
                }
            }
            MemKeychainMode::Working | MemKeychainMode::FailingForget => Ok(slot.clone()),
        }
    }

    fn forget(&self) -> Result<(), KeychainError> {
        if self.mode == MemKeychainMode::FailingForget {
            return Err(KeychainError::Failed("the keychain is locked".to_owned()));
        }
        *self.slot() = None;
        Ok(())
    }
}
