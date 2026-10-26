//! "Stay signed in" on the computer's credential store (ruling H11): `OsKeychain`'s mapping of
//! the store's answers, on `keyring_core`'s mock store, never the real one. One test binary of
//! its own, because the mock is installed as the process's default store.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test: a failure should stop it"
)]

use keyring_core::{Entry, Error, mock};
use tc_app::account::{Keychain, KeychainError, OsKeychain};
use tc_door43::Token;

fn token(secret: &str) -> Token {
    Token::new(secret.to_owned()).unwrap()
}

/// The mock credential behind the keychain's entry, to make its next call fail.
fn fail_next(error: Error) {
    let entry = Entry::new("translationCore 4", "Door43").unwrap();
    let cred: &mock::Cred = entry.as_any().downcast_ref().unwrap();
    cred.set_error(error);
}

#[test]
fn the_token_is_kept_read_and_forgotten_and_each_failure_is_named() {
    keyring_core::set_default_store(mock::Store::new().unwrap());
    let keychain = OsKeychain::over_default_store();
    assert_eq!(keychain.read(), Ok(None), "nothing kept yet");
    keychain.keep(&token("s3cret")).unwrap();
    assert_eq!(keychain.read(), Ok(Some(token("s3cret"))));
    keychain.forget().unwrap();
    assert_eq!(keychain.read(), Ok(None));
    keychain
        .forget()
        .expect("forgetting nothing is not a failure");

    // No store this program may use: "no keychain", so the sign-in hides "Stay signed in".
    keychain.keep(&token("s3cret")).unwrap();
    fail_next(Error::NoStorageAccess(Box::new(std::io::Error::other(
        "locked",
    ))));
    assert_eq!(keychain.read(), Err(KeychainError::NoKeychain));
    // Bytes that do not open are removed and named unreadable, as tC4's readToken does.
    fail_next(Error::BadEncoding(vec![0xff, 0xfe]));
    assert!(matches!(keychain.read(), Err(KeychainError::Unreadable(_))));
    assert_eq!(keychain.read(), Ok(None), "the unreadable bytes are gone");
    // Any other refusal is a failure with the store's own words.
    fail_next(Error::PlatformFailure(Box::new(std::io::Error::other(
        "denied",
    ))));
    assert!(matches!(
        keychain.keep(&token("s3cret")),
        Err(KeychainError::Failed(_))
    ));
}
