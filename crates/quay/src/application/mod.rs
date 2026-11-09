pub mod ports;
pub mod service;

/// Something went wrong in a way worth telling the user, in words they can act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuayError(pub String);

impl QuayError {
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl std::fmt::Display for QuayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
