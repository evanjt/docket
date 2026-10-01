//! Why a command stops: a refusal the person reads, with the exit code the command ends on.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fail {
    /// The rules, the server or the checkout refuse it: the text on stderr, exit 1.
    Refused(String),
}

impl Fail {
    pub fn refused(text: impl Into<String>) -> Self {
        Fail::Refused(text.into())
    }
}

impl fmt::Display for Fail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fail::Refused(text) => f.write_str(text),
        }
    }
}

impl From<docket_core::item::Refused> for Fail {
    fn from(r: docket_core::item::Refused) -> Self {
        Fail::Refused(r.0)
    }
}

pub type Result<T> = std::result::Result<T, Fail>;
