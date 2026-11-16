//! What a use case can refuse (port of `quire/application/errors.py`): English text, also the translation key.

use std::fmt;

use crate::domain::ValidationError;

#[derive(Clone, Debug, PartialEq)]
pub enum AppError {
    /// The thing asked for doesn't exist (any more).
    NotFound(String),
    /// The input broke a rule.
    Invalid(&'static str),
    /// Storage failed underneath.
    Storage(String),
    /// Sync isn't set up on this device yet.
    SyncNotSetUp(String),
    /// The cloud refused the saved sign-in: set up again.
    CloudAuth(String),
    /// The network or the cloud failed; try again later.
    Sync(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(m) | Self::Storage(m) | Self::SyncNotSetUp(m) | Self::CloudAuth(m) | Self::Sync(m) => f.write_str(m),
            Self::Invalid(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for AppError {}

impl From<ValidationError> for AppError {
    fn from(e: ValidationError) -> Self {
        Self::Invalid(e.0)
    }
}

pub type AppResult<T = ()> = Result<T, AppError>;
