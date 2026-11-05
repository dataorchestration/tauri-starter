use super::{DistanceMetric, VectorDocument, VectorMatch};
use crate::db::{Database, DbError};

const UNAVAILABLE: &str =
    "vector search requires the Turso backend; configure with --database turso";

pub async fn create_collection(_: &Database, _: &str, _: u32) -> Result<(), DbError> {
    Err(DbError::Unsupported(UNAVAILABLE))
}
pub async fn upsert(_: &Database, _: &VectorDocument) -> Result<(), DbError> {
    Err(DbError::Unsupported(UNAVAILABLE))
}
pub async fn get(_: &Database, _: &str, _: &str) -> Result<Option<VectorDocument>, DbError> {
    Err(DbError::Unsupported(UNAVAILABLE))
}
pub async fn search(
    _: &Database,
    _: &str,
    _: &[f32],
    _: u32,
    _: DistanceMetric,
) -> Result<Vec<VectorMatch>, DbError> {
    Err(DbError::Unsupported(UNAVAILABLE))
}
pub async fn delete(_: &Database, _: &str, _: &str) -> Result<bool, DbError> {
    Err(DbError::Unsupported(UNAVAILABLE))
}
pub async fn delete_collection(_: &Database, _: &str) -> Result<bool, DbError> {
    Err(DbError::Unsupported(UNAVAILABLE))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn sqlcipher_reports_vectors_unavailable() {
        let db = Database::open_in_memory().await.unwrap();
        assert!(matches!(
            create_collection(&db, "notes", 3).await,
            Err(DbError::Unsupported(_))
        ));
        assert!(matches!(
            search(&db, "notes", &[1.0, 0.0, 0.0], 10, DistanceMetric::Cosine).await,
            Err(DbError::Unsupported(_))
        ));
    }
}
