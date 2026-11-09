//! A rule the user's input broke. The message is the English text, which is also the key the translations use.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationError(pub &'static str);

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for ValidationError {}

pub type Checked<T = ()> = Result<T, ValidationError>;
