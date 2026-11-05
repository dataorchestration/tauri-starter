# Architecture

```
┌──────────────── webview (React 19 + TS) ─────────────────┐
│  components/*  ──▶  store/app.ts (zustand)  ──▶  lib/ipc.ts │
│       ▲                    │                         │       │
│       └── lib/events.ts ◀──┘ (starter://* events)    │ invoke │
└──────────────────────────────────────────────────────┼───────┘
                                                       ▼
┌──────────────── Rust core (src-tauri) ────────────────────────┐
│ commands.rs  (thin IPC surface; IpcError { code, message })    │
│   ├─ db/        selected DB · key.rs (OS keychain) · CRUD/vectors  │
│   ├─ telemetry/ tracing → file log · Sink trait → sentry/http │
│   ├─ crash.rs   panic hook → crashes/*.json → send/dismiss     │
│   ├─ permissions.rs / health.rs                                │
│   ├─ menu.rs    app menu + tray (i18n labels)                  │
│   ├─ updater.rs launch + manual checks, starter://update-ready │
│   └─ i18n.rs    same locale JSON as the webview                │
│ config.rs ◀── starter.config.json ──▶ vite define (__STARTER_CONFIG__) │
└────────────────────────────────────────────────────────────────┘
```

## Startup sequence (`lib.rs::run`)

1. Plugins. `single-instance` must be first. Desktop-only plugins are gated by
   `#[cfg(desktop)]` and by `features.*` in `starter.config.json`.
2. `setup`: resolve `AppPaths` → load/create the DB key from the keychain →
   open the selected encrypted DB (or import into Turso) (runs migrations) → load settings.
   Tauri setup uses one `block_on` bridge before accepting commands.
3. Telemetry **before** the crash hook, so Sentry's panic integration is the
   "previous hook" ours chains to.
4. Menu + tray with the resolved locale. `AppState` is managed for commands.
5. Launch-time update check (only when configured).

## State and threading

- `AppState { db: tokio::sync::RwLock<Option<Database>>, paths, telemetry, meta }`.
  A command holds a read guard through its database operation. Erase takes the
  write guard, drains in-flight operations, closes the engine, removes both
  stores, rotates the key, and opens a fresh store. On failure the state stays
  unavailable (`None`), never an accidentally writable in-memory fallback.
- `Database` is selected at compile time: `sqlcipher_store.rs` (default) or
  `turso_store.rs`. Shared wire models/tests live in users/settings/notifications;
  backend SQL implementations live in their respective subdirectories.
- SQLCipher operations use `spawn_blocking` with an owned async connection guard.
  A cancelled command's worker retains the guard until completion; close/reset
  waits for it. Turso executes queries with native async/await. Both serialize
  access; neither promises concurrent writers. Transactions roll back on drop.
- Vector commands keep stable wire names: Turso implements them, while SQLCipher
  returns `feature_unavailable`. Frontend callers can inspect `app_info.database`.
  OS keychain/file calls remain synchronous.
- Telemetry runs one worker thread (`std::sync::mpsc`), batching by count and
  interval. Sinks are called from that thread only.

## Data on disk

| Path (macOS example) | What |
| --- | --- |
| `~/Library/Application Support/<App>/app.turso.sqlite` | encrypted Turso DB (+ `-wal`) |
| `~/Library/Application Support/<App>/app.sqlite` | active SQLCipher DB, or retained snapshot after Turso import |
| `~/Library/Application Support/<App>/crashes/*.json` | pending crash reports |
| `~/Library/Logs/<App>/app.YYYY-MM-DD.log` | rolling logs (7 by default) |
| macOS Keychain, service = bundle identifier, account `db-key` | 32-byte DB key (hex) |

Windows: `%APPDATA%\<identifier>`, Credential Manager. Linux: `~/.local/share/<identifier>`, Secret Service.

## Wire contracts

- Rust structs use `snake_case` fields; TS mirrors in `src/lib/ipc.ts` match.
  Enums are `#[serde(rename_all = "snake_case")]`.
- Errors: `IpcError { code: &'static str, message }`. Stable codes: `not_found`,
  `db`, `db_wrong_key`, `keychain`, `validation`, `feature_unavailable`, `updater_not_configured`,
  `updater_failed`, `autostart`, `opener`, `telemetry`, `forbidden`, `tauri`.
- Events (core → UI): `starter://navigate {view}`, `starter://update-ready "x.y.z"`,
  `starter://locale-changed "es"`, `starter://notification {Notification}`.

## Adding things

**A command**
1. `commands.rs`: `#[tauri::command] pub async fn thing(state: State<'_, AppState>, arg: T) -> IpcResult<U>`
2. `lib.rs`: add to `generate_handler![...]`
3. `src/lib/ipc.ts`: `thing: (arg: T) => call<U>("thing", { arg })` + types
4. Test the pure logic in the module it calls; UI tests mock `invoke` via `src/test/harness.ts`.

**A table** — `db/migrations/0003_<name>.sql`, append to `MIGRATIONS` in
the selected backend store module, add `db/<name>.rs` with functions taking `&Database` (see
`users.rs`), unit-test with `Database::open_in_memory().await`.

**A setting** — add the field to `AppSettings` (`db/settings.rs`, with a default)
and to the TS interface; `save_settings` applies side effects.

**A permission** — add the variant to `permissions::Permission`, handle it in
each `platform` module (status / request / settings_url), add
`permissions.names.<id>` to every locale.

**A telemetry sink** — implement `telemetry::Sink`, construct it in
`telemetry::init` behind a config check (and a Cargo feature if it pulls deps).

**A language** — `src/i18n/locales/<code>.json`; import in `src/i18n/index.ts`
and `src-tauri/src/i18n.rs`; add to `starter.config.json`. Tests on both sides
fail if a key is missing.

**A window** — add it to `tauri.conf.json` → `app.windows`, grant it a
capability in `src-tauri/capabilities/`, and give it its own HTML entry in
`vite.config.ts` (`build.rollupOptions.input`).

## Security posture

- The DB key is held in the OS keychain, not written to an application file.
  SQLCipher is the default; optional Turso uses AEGIS-256 authenticated page
  encryption (experimental upstream).
  See [DATABASE.md](DATABASE.md) for migration and encryption limits.
- The webview gets only what commands return. Nothing in `src/` reads files or
  secrets directly.
- CSP is set in `tauri.conf.json`; capabilities grant only the plugins used.
- Telemetry is opt-in, content-free, and carries a random per-install id (rotated
  by "erase all data"). Crash reports are sent only on explicit user action.
