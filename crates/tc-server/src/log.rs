//! One stderr line for what the person or a developer should hear about. The token, a
//! header value and a URI with its query never go in one (APP-DESIGN §5.1).

pub(crate) fn warn(message: impl std::fmt::Display) {
    eprintln!("tc4: {message}");
}
