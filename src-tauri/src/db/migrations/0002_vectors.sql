-- Collections bind one embedding model/dimension to a namespace. Different
-- models should always use different collections, even at the same dimension.
CREATE TABLE vector_collections (
    name TEXT PRIMARY KEY,
    dimensions INTEGER NOT NULL CHECK (dimensions BETWEEN 1 AND 4096)
);
CREATE TABLE vector_documents (
    collection TEXT NOT NULL REFERENCES vector_collections(name) ON DELETE CASCADE,
    id TEXT NOT NULL,
    embedding F32_BLOB NOT NULL,
    metadata TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (collection, id)
);
