use crate::schema::record::{CanonicalEdge, CanonicalNode};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GraphSnapshot {
    pub nodes: BTreeMap<String, CanonicalNode>,
    pub edges: BTreeMap<String, CanonicalEdge>,
}

impl GraphSnapshot {
    pub fn assert_equal(&self, other: &Self) {
        let s1 = serde_json::to_string_pretty(self).expect("Failed to serialize snapshot 1");
        let s2 = serde_json::to_string_pretty(other).expect("Failed to serialize snapshot 2");
        assert_eq!(s1, s2, "Snapshots are not identical");
    }
}
