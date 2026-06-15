#![expect(missing_docs, reason = "We'll do it later.")]

use std::{error, fmt};

/// Indicates that an error occurred when accessing a reloadable layer.
#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
}

#[derive(Debug)]
pub(crate) enum ErrorKind {
    SubscriberGone,
    SubscriberNotInitialized,
    Poisoned,
}

impl Error {
    pub(crate) fn subscriber_gone() -> Self {
        Self {
            kind: ErrorKind::SubscriberGone,
        }
    }

    pub(crate) fn subscriber_not_initialized() -> Self {
        Self {
            kind: ErrorKind::SubscriberNotInitialized,
        }
    }

    pub(crate) fn poisoned() -> Self {
        Self {
            kind: ErrorKind::Poisoned,
        }
    }

    #[must_use]
    pub fn is_poisoned(&self) -> bool {
        matches!(self.kind, ErrorKind::Poisoned)
    }

    #[must_use]
    pub fn is_gone(&self) -> bool {
        matches!(self.kind, ErrorKind::SubscriberGone)
    }

    #[must_use]
    pub fn is_uninitialized(&self) -> bool {
        matches!(self.kind, ErrorKind::SubscriberNotInitialized)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self.kind {
            ErrorKind::SubscriberGone => "subscriber no longer exists",
            ErrorKind::SubscriberNotInitialized => "subscriber was not initialized",
            ErrorKind::Poisoned => "lock poisoned",
        };
        f.pad(msg)
    }
}

impl error::Error for Error {}
