//! Vector wire types are stable in both builds. SQLCipher reports unavailable.
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_DIMENSIONS: usize = 4096;
pub const MAX_RESULTS: u32 = 100;

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DistanceMetric {
    #[default]
    Cosine,
    Euclidean,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct VectorDocument {
    pub collection: String,
    pub id: String,
    pub embedding: Vec<f32>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Serialize)]
pub struct VectorMatch {
    pub id: String,
    pub metadata: Value,
    /// Distance, not similarity: smaller is closer for either metric.
    pub distance: f64,
}

#[cfg(feature = "database-turso")]
#[path = "turso/vectors.rs"]
mod backend;
#[cfg(feature = "database-sqlcipher")]
#[path = "sqlcipher/vectors.rs"]
mod backend;
pub use backend::{create_collection, delete, delete_collection, get, search, upsert};
