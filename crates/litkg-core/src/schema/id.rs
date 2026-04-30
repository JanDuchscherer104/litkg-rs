use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StableId(String);

impl StableId {
    pub fn new(id: impl Into<String>) -> Self {
        // TODO: Add format validation
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for StableId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for StableId {
    fn from(id: String) -> Self {
        Self::new(id)
    }
}

impl From<&str> for StableId {
    fn from(id: &str) -> Self {
        Self::new(id)
    }
}
