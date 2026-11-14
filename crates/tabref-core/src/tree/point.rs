use serde::{Deserialize, Serialize};

/// A point in CSS pixels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
