//! Exact vector search using Turso's built-in vector32 / distance functions.
//! No extension, cloud dependency, or embedding provider. See docs/DATABASE.md.

use super::{Database, DbError, now_millis};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use turso::{Connection, params};

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

fn validate_name(value: &str) -> Result<(), DbError> {
    if value.trim().is_empty() || value.len() > 256 {
        return Err(DbError::Validation(
            "collection and document IDs must contain 1–256 bytes".into(),
        ));
    }
    Ok(())
}

fn validate_vector(vector: &[f32], dimensions: usize) -> Result<(), DbError> {
    if vector.len() != dimensions || dimensions == 0 || dimensions > MAX_DIMENSIONS {
        return Err(DbError::Validation(format!(
            "embedding must have {dimensions} dimensions (1–{MAX_DIMENSIONS})"
        )));
    }
    let norm: f64 = vector.iter().map(|&v| f64::from(v).powi(2)).sum();
    // Avoid zero/underflow for cosine and overflow in float32 distances.
    if !vector.iter().all(|v| v.is_finite())
        || !(f64::from(f32::MIN_POSITIVE)..=f64::from(f32::MAX) / 4.0).contains(&norm)
    {
        return Err(DbError::Validation(
            "embedding must be finite and have a nonzero, float32-safe norm".into(),
        ));
    }
    Ok(())
}

async fn dimensions(conn: &Connection, collection: &str) -> Result<usize, DbError> {
    validate_name(collection)?;
    let mut rows = conn
        .query(
            "SELECT dimensions FROM vector_collections WHERE name = ?1",
            [collection],
        )
        .await?;
    Ok(rows.next().await?.ok_or(DbError::NotFound)?.get::<i64>(0)? as usize)
}

/// Idempotent for the same dimensions; changing a collection's model/dimension
/// requires a different name or an explicit delete/recreate.
pub async fn create_collection(db: &Database, name: &str, size: u32) -> Result<(), DbError> {
    validate_name(name)?;
    if size == 0 || size as usize > MAX_DIMENSIONS {
        return Err(DbError::Validation(format!(
            "dimensions must be 1–{MAX_DIMENSIONS}"
        )));
    }
    let conn = db.connection().await;
    conn.execute("INSERT INTO vector_collections (name, dimensions) VALUES (?1, ?2) ON CONFLICT(name) DO NOTHING", params![name, i64::from(size)]).await?;
    if dimensions(&conn, name).await? != size as usize {
        return Err(DbError::Validation(
            "collection already has different dimensions".into(),
        ));
    }
    Ok(())
}

pub async fn upsert(db: &Database, document: &VectorDocument) -> Result<(), DbError> {
    validate_name(&document.id)?;
    let conn = db.connection().await;
    let size = dimensions(&conn, &document.collection).await?;
    validate_vector(&document.embedding, size)?;
    conn.execute(
        "INSERT INTO vector_documents (collection, id, embedding, metadata, updated_at)
         VALUES (?1, ?2, vector32(?3), ?4, ?5)
         ON CONFLICT(collection, id) DO UPDATE SET embedding = excluded.embedding,
         metadata = excluded.metadata, updated_at = excluded.updated_at",
        params![
            document.collection.clone(),
            document.id.clone(),
            serde_json::to_string(&document.embedding)?,
            serde_json::to_string(&document.metadata)?,
            now_millis()
        ],
    )
    .await?;
    Ok(())
}

pub async fn get(
    db: &Database,
    collection: &str,
    id: &str,
) -> Result<Option<VectorDocument>, DbError> {
    validate_name(collection)?;
    validate_name(id)?;
    let conn = db.connection().await;
    let mut rows = conn.query("SELECT vector_extract(embedding), metadata FROM vector_documents WHERE collection = ?1 AND id = ?2", params![collection, id]).await?;
    let Some(row) = rows.next().await? else {
        return Ok(None);
    };
    Ok(Some(VectorDocument {
        collection: collection.into(),
        id: id.into(),
        embedding: serde_json::from_str(&row.get::<String>(0)?)?,
        metadata: serde_json::from_str(&row.get::<String>(1)?)?,
    }))
}

pub async fn search(
    db: &Database,
    collection: &str,
    embedding: &[f32],
    limit: u32,
    metric: DistanceMetric,
) -> Result<Vec<VectorMatch>, DbError> {
    if limit == 0 || limit > MAX_RESULTS {
        return Err(DbError::Validation(format!(
            "limit must be 1–{MAX_RESULTS}"
        )));
    }
    let conn = db.connection().await;
    validate_vector(embedding, dimensions(&conn, collection).await?)?;
    // The only interpolated value is selected from this enum, never user SQL.
    let function = match metric {
        DistanceMetric::Cosine => "vector_distance_cos",
        DistanceMetric::Euclidean => "vector_distance_l2",
    };
    let mut rows = conn
        .query(
            format!(
                "SELECT id, metadata, {function}(embedding, vector32(?2)) AS distance
         FROM vector_documents WHERE collection = ?1 ORDER BY distance ASC, id ASC LIMIT ?3"
            ),
            params![
                collection,
                serde_json::to_string(embedding)?,
                i64::from(limit)
            ],
        )
        .await?;
    let mut matches = Vec::new();
    while let Some(row) = rows.next().await? {
        matches.push(VectorMatch {
            id: row.get(0)?,
            metadata: serde_json::from_str(&row.get::<String>(1)?)?,
            distance: row.get(2)?,
        });
    }
    Ok(matches)
}

pub async fn delete(db: &Database, collection: &str, id: &str) -> Result<bool, DbError> {
    validate_name(collection)?;
    validate_name(id)?;
    Ok(db
        .connection()
        .await
        .execute(
            "DELETE FROM vector_documents WHERE collection = ?1 AND id = ?2",
            params![collection, id],
        )
        .await?
        > 0)
}

pub async fn delete_collection(db: &Database, collection: &str) -> Result<bool, DbError> {
    validate_name(collection)?;
    Ok(db
        .connection()
        .await
        .execute(
            "DELETE FROM vector_collections WHERE name = ?1",
            [collection],
        )
        .await?
        > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(collection: &str, id: &str, embedding: Vec<f32>) -> VectorDocument {
        VectorDocument {
            collection: collection.into(),
            id: id.into(),
            embedding,
            metadata: serde_json::json!({"text": "a 'quoted' document"}),
        }
    }

    #[tokio::test]
    async fn native_distances_upserts_namespaces_and_cascade() {
        let db = Database::open_in_memory().await.unwrap();
        for name in ["model-v1", "other's model"] {
            create_collection(&db, name, 2).await.unwrap();
        }
        create_collection(&db, "model-v1", 2).await.unwrap();
        for (id, vector) in [
            ("near", vec![1.0, 0.0]),
            ("orthogonal", vec![0.0, 1.0]),
            ("opposite", vec![-1.0, 0.0]),
        ] {
            upsert(&db, &document("model-v1", id, vector))
                .await
                .unwrap();
        }
        upsert(&db, &document("other's model", "foreign", vec![1.0, 0.0]))
            .await
            .unwrap();
        for (metric, middle) in [
            (DistanceMetric::Cosine, 1.0),
            (DistanceMetric::Euclidean, 2.0f64.sqrt()),
        ] {
            let hits = search(&db, "model-v1", &[1.0, 0.0], 3, metric)
                .await
                .unwrap();
            assert_eq!(
                hits.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(),
                ["near", "orthogonal", "opposite"]
            );
            assert!(hits[0].distance.abs() < 1e-6);
            assert!((hits[1].distance - middle).abs() < 1e-6);
            assert!((hits[2].distance - 2.0).abs() < 1e-6);
        }
        let changed = document("model-v1", "near", vec![0.0, -1.0]);
        upsert(&db, &changed).await.unwrap();
        assert_eq!(
            get(&db, "model-v1", "near").await.unwrap().unwrap(),
            changed
        );
        assert_eq!(
            search(&db, "model-v1", &[1.0, 0.0], 1, DistanceMetric::Cosine)
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(delete(&db, "model-v1", "near").await.unwrap());
        assert!(!delete(&db, "model-v1", "near").await.unwrap());
        assert!(delete_collection(&db, "model-v1").await.unwrap());
        assert!(get(&db, "model-v1", "orthogonal").await.unwrap().is_none());
        assert!(
            get(&db, "other's model", "foreign")
                .await
                .unwrap()
                .is_some()
        );
        create_collection(&db, "model-v1", 3).await.unwrap();
        assert!(
            search(
                &db,
                "model-v1",
                &[1.0, 0.0, 0.0],
                10,
                DistanceMetric::Cosine
            )
            .await
            .unwrap()
            .is_empty()
        );
    }

    #[tokio::test]
    async fn rejects_invalid_vectors_dimensions_and_limits() {
        let db = Database::open_in_memory().await.unwrap();
        for size in [0, 4097] {
            assert!(create_collection(&db, "bad", size).await.is_err());
        }
        assert!(create_collection(&db, " ", 2).await.is_err());
        create_collection(&db, "test", 2).await.unwrap();
        assert!(create_collection(&db, "test", 3).await.is_err());
        for vector in [
            vec![],
            vec![1.0],
            vec![0.0, 0.0],
            vec![f32::NAN, 0.0],
            vec![f32::INFINITY, 0.0],
            vec![f32::MAX, 1.0],
            vec![f32::MIN_POSITIVE, 0.0],
        ] {
            assert!(
                upsert(&db, &document("test", "bad", vector.clone()))
                    .await
                    .is_err()
            );
            assert!(
                search(&db, "test", &vector, 1, DistanceMetric::Cosine)
                    .await
                    .is_err()
            );
        }
        for limit in [0, 101] {
            assert!(
                search(&db, "test", &[1.0, 0.0], limit, DistanceMetric::Cosine)
                    .await
                    .is_err()
            );
        }
        assert!(matches!(
            upsert(&db, &document("missing", "id", vec![1.0, 0.0])).await,
            Err(DbError::NotFound)
        ));
    }

    #[tokio::test]
    async fn embeddings_persist_in_encrypted_store() {
        let path = super::super::temp_db_path("vectors");
        let key = super::super::key::generate_hex();
        let db = Database::open(&path, &key).await.unwrap();
        create_collection(&db, "persist", 3).await.unwrap();
        let original = document("persist", "doc", vec![0.25, 0.5, 1.0]);
        upsert(&db, &original).await.unwrap();
        db.checkpoint().await.unwrap();
        drop(db);
        let db = Database::open(&path, &key).await.unwrap();
        assert_eq!(get(&db, "persist", "doc").await.unwrap(), Some(original));
        assert_eq!(
            search(&db, "persist", &[0.25, 0.5, 1.0], 1, DistanceMetric::Cosine)
                .await
                .unwrap()[0]
                .id,
            "doc"
        );
        drop(db);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
