use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StableId(String);

impl StableId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.0.trim().is_empty() {
            return Err("stable id must not be empty".into());
        }
        if self.0.chars().any(char::is_whitespace) {
            return Err(format!(
                "stable id '{}' must not contain whitespace",
                self.0
            ));
        }
        Ok(())
    }

    pub fn is_valid(&self) -> bool {
        self.validate().is_ok()
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
