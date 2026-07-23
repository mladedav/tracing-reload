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

    /// This signals that the error happened because we tried to lock a poisoned mutex, signalling
    /// that a panic most likely occurred previously on another thread inside a this library.
    #[must_use]
    pub fn is_poisoned(&self) -> bool {
        matches!(self.kind, ErrorKind::Poisoned)
    }

    /// This signals that the error happened because someone called an operation after the
    /// subscriber has been deregistered. This will most likely happen when someone tries to reload
    /// a layer on a subscriber that no longer exists.
    #[must_use]
    pub fn is_gone(&self) -> bool {
        matches!(self.kind, ErrorKind::SubscriberGone)
    }

    /// This signals that the error happened because someone called an operation before the
    /// subscriber has been registered. This will most likely happen when someone tries to reload a
    /// layer on a subscriber that was not yet registered as a [`Dispatch`](tracing::Dispatch).
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
