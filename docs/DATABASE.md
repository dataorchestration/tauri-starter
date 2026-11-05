# Choosing local storage

The template defaults to encrypted **SQLCipher**. **Turso** is optional, and its
SQLCipher importer is a separate opt-in. Select once while configuring a derived
project; this is not an end-user switch in Settings.

```sh
npm run configure                         # interactive identity + database prompts
npm run configure -- --database sqlcipher
npm run configure -- --database turso --import-sqlcipher false
npm run configure -- --database turso --import-sqlcipher true
```

Non-interactive identity changes preserve the saved selection. Choosing SQLCipher
clears the import option unless an explicit, invalid import request was supplied.
Invalid choices are rejected before setup rewrites project files.

| Selection | Cargo feature | Database file | Vectors | SQLCipher dependency |
| --- | --- | --- | --- | --- |
| SQLCipher | `database-sqlcipher` | `app.sqlite` | Unavailable | Yes |
| Turso | `database-turso` | `app.turso.sqlite` | Exact cosine / Euclidean | No |
| Turso with import | `database-turso`, `legacy-sqlcipher-import` | `app.turso.sqlite` | Same | Only for import |

`starter.config.json` records `database.engine` and `database.importSqlcipher`.
Configure generates matching default Cargo features and preserves unrelated
features such as telemetry. `build.rs` rejects no-engine, both-engine, and
config/feature mismatches; use configure rather than editing just Cargo features.
The Rust and TypeScript config mirrors and `app_info.database` expose the choice.

Turso's Rust engine is pinned to **0.8.3**, under the
[MIT license](https://github.com/tursodatabase/turso/blob/main/LICENSE.md).
Neither backend requires a database server, cloud account, URL, or token.
The `turso` crate is distinct from `libsql`.

## Async execution and encryption

Both selections expose the same async CRUD/IPC API. SQLCipher is a synchronous
engine: operations run on blocking workers. An owned async lock stays inside the
worker, so cancellation of its caller cannot let reset delete an in-use file.
Turso uses native async/await for opening, executing, querying, and reading rows.
It uses an async connection mutex. Both backends intentionally serialize DB
operations; neither configuration promises parallel searches or concurrent writers.

The synchronous Tauri setup hook bridges once with `block_on` before commands
are accepted. OS keychain/file calls still use synchronous APIs. Async queries
are not free of CPU cost; Turso advertises `io_uring` on Linux, with platform
backends on macOS/Windows. Benchmark actual data and target devices.

Both stores use a random 256-bit key from the OS keychain. SQLCipher uses its
existing raw-key encryption; Turso uses AEGIS-256 page encryption. Turso remains
pre-1.0 and **its encryption is explicitly experimental upstream**. There is no
plaintext-file fallback. Tests exercise encrypted persistence, wrong-key
rejection, and absence of a plaintext marker in the main file/WAL. Tests are not
a security audit; the health check's header check is only a sanity check.

Sources: [Rust API](https://docs.rs/crate/turso/0.8.3),
[Turso features and roadmap](https://github.com/tursodatabase/turso#features-and-roadmap).

## Existing data and migration

- Selecting a backend changes the build; configure does not open or convert any
  user's database. Import happens when the configured application starts.
- A Turso build without import support refuses startup when only `app.sqlite`
  exists. It does not silently create an empty replacement. Reconfigure with
  `--database turso --import-sqlcipher true` to enable the importer.
- The importer opens SQLCipher read-only using the existing key, snapshots stock
  v1 users/settings/notifications and their sequence (including committed WAL
  data), then copies them transactionally into an encrypted staging database.
  IDs, nullable fields, JSON settings/device ID, read flags, and timestamps survive.
- Only after checkpointing and file sync is staging renamed to the new store.
  Interrupted staging is discarded and retried on the next launch. The original
  encrypted SQLCipher files remain as a recovery snapshot, with no plaintext export.
- Wrong keys, unsupported versions, or any deviation from the shipped v1 schema
  stop import. Extend the importer for a starter-derived application's schema.
- SQLCipher builds refuse startup if a Turso database already exists: reopening
  the old SQLCipher snapshot would silently hide changes made after migration.
  Reverse migration is not implemented. Do not run both builds on one data directory.
- Successful migration does not keep the retained SQLCipher snapshot in sync.
  Keep it and the key available for recovery. Import support may be removed from
  later builds once all supported installs have migrated.
- “Erase all data” drains operations (including cancelled SQLCipher callers'
  workers), closes the engine, removes both stores and staging/sidecar files, and
  rotates the key. Errors leave the store unavailable until retry or recovery.

Turso cannot directly open SQLCipher encryption. SQLCipher/OpenSSL are compiled
only for the SQLCipher selection or explicit legacy import; those Windows builds
still require Perl/NASM. A fresh Turso project excludes that dependency.

## Vector API

This API is available only with `--database turso`. SQLCipher builds retain the
same command names and return `feature_unavailable`; they do not emulate vector
search or load a vector extension. Inspect `(await api.appInfo()).database.engine`
or `config.database.engine` before showing vector functionality.

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
Each backend has its own migration list (`sqlcipher_store.rs`, `turso_store.rs`).
Extend the importer when adding application-specific schemas. Migration version
changes occur in the same transaction as their schema changes.
A future schema version is rejected, and a failed migration is rolled back.

Use `#[tokio::test]` with `Database::open_in_memory().await` for logic, plus
file-backed encrypted tests for persistence. Run `npm run check` and
`npm run tauri build -- --bundles app`; CI checks the default and both Turso selections on macOS, Windows, and Linux,
without bundle builds. Local bundle builds or the manual Build app workflow
remain the integration gate.
