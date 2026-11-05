# Local Turso database

The application uses the **Turso Rust engine**, pinned to `turso = 0.8.3`,
under the [MIT license](https://github.com/tursodatabase/turso/blob/main/LICENSE.md).
It is an embedded database: no Turso account, URL, token, server, or network
connection is needed. This is the `turso` crate, distinct from `libsql`.

## Async execution and maturity

Opening, executing, querying, and reading rows use native Rust async/await.
Database commands run on Tauri's async runtime. Async locks protect the store
lifecycle and a single shared connection; they do not block a thread while
waiting for another operation. Operations are serialized, not concurrent writers.
The synchronous Tauri `setup` hook bridges once with `block_on` before commands
are accepted. The legacy importer alone uses a blocking worker for SQLCipher.

Async APIs do not make a full vector scan free of CPU cost or guarantee kernel
async I/O on every OS. Turso advertises `io_uring` on Linux; macOS/Windows use
its platform backend. Benchmark the actual dataset and target devices.

Turso remains pre-1.0, and **encryption is explicitly experimental upstream**.
The starter enables its AEGIS-256 page encryption with a random 256-bit key from
the existing OS keychain entry. It never falls back to an unencrypted file.
Tests exercise encrypted persistence, absence of a plaintext marker in the main
file/WAL, and rejection of a different key. These tests are not a security audit.
The health check's header test is a sanity check, not proof of encryption.

Sources: [Rust API](https://docs.rs/crate/turso/0.8.3),
[Turso features and roadmap](https://github.com/tursodatabase/turso#features-and-roadmap).

## Upgrading existing installs

- Fresh installs create `app.turso.sqlite` in the app data directory.
- When only `app.sqlite` exists, startup opens it read-only using SQLCipher and
  the existing key, snapshots the stock v1 users/settings/notifications and
  notification sequence, and imports them into an encrypted staging database.
  IDs, nullable fields, JSON settings/device ID, read flags, and timestamps survive.
- Import runs in a transaction. After a successful checkpoint and file sync,
  the staging file is renamed to `app.turso.sqlite`. A crash before publication
  leaves no accepted new database; startup discards that staging file and retries.
- The original SQLCipher file is retained. It is not kept in sync after import.
  Do not run old and new app versions against the same data directory at once.
  Reverting to the old app uses the old snapshot and omits subsequent changes.
- Wrong keys, unsupported schema versions, any deviation from the shipped v1 schema
  (including custom indexes or constraints) stop the import. Starter-derived applications must extend the importer
  for their schema. No plaintext export is written.
- `rusqlite`/SQLCipher remain **only for import**. Consequently Windows still
  needs Perl/NASM for its vendored OpenSSL build. Ordinary CRUD uses Turso.
- “Erase all data” drains active DB commands, closes the engine, removes Turso,
  staging, and retained SQLCipher files and sidecars, then rotates the key. If
  reset fails, commands fail closed; retry erase or restart after fixing the error.

Do not point Turso directly at a SQLCipher-encrypted file: the encryption formats
are different. Keep the database and its key available for recovery.

## Vector API

Embeddings stay inside the same encrypted database. The application supplies
embeddings from its own model; the starter neither generates them nor uploads
text. Use one collection per model/version, even if dimensions match.

```ts
import { api } from "../lib/ipc";

await api.createVectorCollection("notes-model-v1", 3);
await api.upsertVector({
  collection: "notes-model-v1",
  id: "note-1",
  embedding: [0.25, 0.5, 1],
  metadata: { title: "Local notes", tag: "work" },
});

const nearest = await api.searchVectors(
  "notes-model-v1", [0.2, 0.5, 0.9], 10, "cosine",
);
// [{ id, metadata, distance }]; lower distance means closer.
const stored = await api.getVector("notes-model-v1", "note-1");
await api.deleteVector("notes-model-v1", "note-1");
await api.deleteVectorCollection("notes-model-v1"); // also deletes its vectors
```

| Operation | Contract |
| --- | --- |
| Create collection | 1–4096 dimensions; same name/dimensions is idempotent, a dimension change is rejected |
| Upsert/get/delete | Keyed by collection + document ID; metadata is JSON; upsert replaces embedding and metadata |
| Search | `cosine` (default) or `euclidean`; exact top-k, 1–100 results, default 10 |
| Ordering | Ascending distance, then document ID for deterministic ties |
| Validation | Nonempty IDs/names up to 256 bytes; matching dimensions; finite vectors with nonzero, float32-safe norm |
| Errors | `validation` for invalid inputs, `not_found` for missing collections on upsert/search |
| Empty results | Existing empty collection searches to `[]`; a missing document reads as `null` |

Vectors use `F32_BLOB` with native `vector32`, `vector_extract`,
`vector_distance_cos`, and `vector_distance_l2`. SQL values are bound parameters;
the distance function is selected from a Rust enum. The collection's primary
key prefix narrows candidates; there is **no ANN vector index**. Search evaluates
all candidates in the collection (roughly O(rows × dimensions), plus top-k
ordering). Metadata is returned but is not a filter language.

Turso's Rust engine currently lists approximate vector indexing on its roadmap.
Do not copy `libsql_vector_idx`/`vector_top_k` examples from libSQL into this
integration. No ANN, cloud sync, FTS, embedding model, or quality guarantee is
implied by this API. Large corpora need workload-specific benchmarks and an
indexing strategy before release.

## Extending and testing

Append migrations (`0003_*.sql` onward); never edit shipped migration 0001.
Migration version changes occur in the same transaction as their schema changes.
A future schema version is rejected, and a failed migration is rolled back.

Use `#[tokio::test]` with `Database::open_in_memory().await` for logic, plus
file-backed encrypted tests for persistence. Run `npm run check` and
`npm run tauri build -- --bundles app`; CI checks macOS, Windows, and Linux.
