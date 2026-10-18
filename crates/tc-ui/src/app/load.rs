//! What a screen holds for a read (APP-DESIGN 6.2).

use super::client::CallError;

/// The state of one read. "Failed" is never drawn as "absent": there is no variant that is
/// both.
#[derive(Debug, Clone, PartialEq)]
pub enum Load<T> {
    /// The first fetch is running.
    Loading,
    Ready(T),
    /// The value is on screen while a newer one is fetched, so a book that waits behind a
    /// commit shows as pending, never as empty (DELTA 625-626).
    Refreshing(T),
    Failed(CallError),
}

impl<T> Load<T> {
    /// The value on screen: ready or being refreshed.
    pub fn value(&self) -> Option<&T> {
        match self {
            Self::Ready(value) | Self::Refreshing(value) => Some(value),
            Self::Loading | Self::Failed(_) => None,
        }
    }

    /// A fetch is running.
    pub fn is_pending(&self) -> bool {
        matches!(self, Self::Loading | Self::Refreshing(_))
    }

    pub fn error(&self) -> Option<&CallError> {
        match self {
            Self::Failed(error) => Some(error),
            _ => None,
        }
    }

    /// A fetch starts: a value stays on screen as `Refreshing`; `Loading` and `Failed` become
    /// `Loading`, because there is nothing to keep.
    pub fn refreshing(self) -> Self {
        match self {
            Self::Ready(value) | Self::Refreshing(value) => Self::Refreshing(value),
            Self::Loading | Self::Failed(_) => Self::Loading,
        }
    }

    /// A refresh ended without a newer value: what is shown stays, no longer pending.
    pub fn settled(self) -> Self {
        match self {
            Self::Refreshing(value) => Self::Ready(value),
            other => other,
        }
    }

    pub fn as_ref(&self) -> Load<&T> {
        match self {
            Self::Loading => Load::Loading,
            Self::Ready(value) => Load::Ready(value),
            Self::Refreshing(value) => Load::Refreshing(value),
            Self::Failed(error) => Load::Failed(error.clone()),
        }
    }

    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Load<U> {
        match self {
            Self::Loading => Load::Loading,
            Self::Ready(value) => Load::Ready(f(value)),
            Self::Refreshing(value) => Load::Refreshing(f(value)),
            Self::Failed(error) => Load::Failed(error),
        }
    }
}
