//! The keychain trait and its in-memory kind.

use tc_app::account::{Keychain, KeychainError, MemKeychain, MemKeychainMode};
use tc_door43::Token;

pub fn token(text: &str) -> Token {
    Token::new(text.to_owned()).unwrap()
}

#[test]
fn mem_keychain_keeps_reads_and_forgets() {
    let keychain = MemKeychain::new();
    assert_eq!(keychain.read(), Ok(None));
    keychain.keep(&token("abc123")).unwrap();
    assert_eq!(keychain.read().unwrap(), Some(token("abc123")));
    keychain.keep(&token("def456")).unwrap();
    assert_eq!(keychain.read().unwrap(), Some(token("def456")));
    keychain.forget().unwrap();
    assert_eq!(keychain.read(), Ok(None));
    // Forgetting nothing is not an error.
    keychain.forget().unwrap();
}

#[test]
fn without_a_keychain_keep_is_refused() {
    let keychain = MemKeychain::with_mode(MemKeychainMode::NoKeychain);
    assert_eq!(
        keychain.keep(&token("abc123")),
        Err(KeychainError::NoKeychain)
    );
    // As an operating system with no keychain answers: there is none to read.
    assert_eq!(keychain.read(), Err(KeychainError::NoKeychain));
    assert_eq!(keychain.forget(), Ok(()), "nothing to forget");
}

#[test]
fn a_keychain_that_cannot_forget_says_so_and_keeps_the_token() {
    let keychain = MemKeychain::holding(token("abc123"));
    let failing = MemKeychain::with_mode(MemKeychainMode::FailingForget);
    failing.seed(token("abc123"));
    assert!(matches!(failing.forget(), Err(KeychainError::Failed(_))));
    assert_eq!(failing.read().unwrap(), Some(token("abc123")));
    assert!(keychain.forget().is_ok());
}

#[test]
fn unreadable_bytes_are_removed_and_reported_once() {
    let keychain = MemKeychain::with_mode(MemKeychainMode::Unreadable);
    keychain.seed(token("abc123"));
    assert!(matches!(keychain.read(), Err(KeychainError::Unreadable(_))));
    // They are gone: no use to anyone.
    assert_eq!(keychain.read(), Ok(None));
}

#[test]
fn a_token_prints_redacted() {
    let text = format!("{:?}", token("supersecret123"));
    assert!(!text.contains("supersecret123"), "{text}");
}
