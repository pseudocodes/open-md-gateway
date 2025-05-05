use std::fmt;
use serde::{Serialize, Deserialize};

/// Represents an optional numeric value in market data
/// This is needed because market data may contain special
/// string values like "-" to represent missing data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OptionalNumeric<T> {
    /// A valid numeric value
    Value(T),
    /// A string value (usually "-" for missing data)
    String(String),
    /// A null value (explicitly missing)
    #[serde(rename = "null")]
    Null,
}

impl<T: Default> Default for OptionalNumeric<T> {
    fn default() -> Self {
        OptionalNumeric::Null
    }
}

impl<T: fmt::Display> fmt::Display for OptionalNumeric<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            OptionalNumeric::Value(v) => write!(f, "{}", v),
            OptionalNumeric::String(s) => write!(f, "{}", s),
            OptionalNumeric::Null => write!(f, "null"),
        }
    }
}

/// Type alias for optional market data fields (typically price-related)
pub type OptionalF64 = OptionalNumeric<f64>;

/// Type alias for optional volume fields
pub type OptionalI64 = OptionalNumeric<i64>; 