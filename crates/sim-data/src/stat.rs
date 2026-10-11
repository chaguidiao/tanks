use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    High,
    Medium,
    Low,
    Estimate,
}

/// A numeric value with provenance. `source` names a public reference (see docs/REFERENCES.md);
/// `confidence: estimate` values must explain their reasoning in `notes`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stat<T> {
    pub value: T,
    pub source: String,
    pub confidence: Confidence,
    #[serde(default)]
    pub notes: Option<String>,
}

impl<T: Copy> Stat<T> {
    pub fn get(&self) -> T {
        self.value
    }
}

/// In-service date range (years, inclusive). Era is data: a scenario date decides what exists.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ServiceDates {
    pub from: i32,
    #[serde(default)]
    pub to: Option<i32>,
}

impl ServiceDates {
    pub fn contains(&self, year: i32) -> bool {
        year >= self.from && self.to.is_none_or(|t| year <= t)
    }
}
