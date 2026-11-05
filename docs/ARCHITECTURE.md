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
│   ├─ db/        SQLCipher store · key.rs (OS keychain) · CRUD  │
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
   open the encrypted DB (runs migrations) → load settings.
3. Telemetry **before** the crash hook, so Sentry's panic integration is the
   "previous hook" ours chains to.
4. Menu + tray with the resolved locale. `AppState` is managed for commands.
5. Launch-time update check (only when configured).

## State and threading

- `AppState { db: RwLock<Database>, paths, telemetry, meta }`. `Database` holds
  a `Mutex<Connection>`; the outer `RwLock` exists only so "erase all data" can
  swap the database. Commands take `read()`.
- Telemetry runs one worker thread (`std::sync::mpsc`), batching by count and
  interval. Sinks are called from that thread only.
- Commands are sync unless they await (updater). Keep DB work inside the
  closure passed to `Database::with` so the lock is short.

## Data on disk

| Path (macOS example) | What |
| --- | --- |
| `~/Library/Application Support/<App>/app.sqlite` | SQLCipher DB (+ `-wal`, `-shm`) |
| `~/Library/Application Support/<App>/crashes/*.json` | pending crash reports |
| `~/Library/Logs/<App>/app.YYYY-MM-DD.log` | rolling logs (7 by default) |
| macOS Keychain, service = bundle identifier, account `db-key` | 32-byte DB key (hex) |

Windows: `%APPDATA%\<identifier>`, Credential Manager. Linux: `~/.local/share/<identifier>`, Secret Service.

## Wire contracts

- Rust structs use `snake_case` fields; TS mirrors in `src/lib/ipc.ts` match.
  Enums are `#[serde(rename_all = "snake_case")]`.
- Errors: `IpcError { code: &'static str, message }`. Stable codes: `not_found`,
  `db`, `db_wrong_key`, `keychain`, `validation`, `updater_not_configured`,
  `updater_failed`, `autostart`, `opener`, `telemetry`, `forbidden`, `tauri`.
- Events (core → UI): `starter://navigate {view}`, `starter://update-ready "x.y.z"`,
  `starter://locale-changed "es"`, `starter://notification {Notification}`.

## Adding things

**A command**
1. `commands.rs`: `#[tauri::command] pub fn thing(state: State<AppState>, arg: T) -> IpcResult<U>`
2. `lib.rs`: add to `generate_handler![...]`
3. `src/lib/ipc.ts`: `thing: (arg: T) => call<U>("thing", { arg })` + types
4. Test the pure logic in the module it calls; UI tests mock `invoke` via `src/test/harness.ts`.

**A table** — `db/migrations/0002_<name>.sql`, append to `MIGRATIONS` in
`db/mod.rs`, add `db/<name>.rs` with functions taking `&Database` (see
`users.rs`), unit-test with `Database::open_in_memory()`.

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

- The DB key never touches disk; the DB file is useless without the keychain
  entry of the same OS account.
- The webview gets only what commands return. Nothing in `src/` reads files or
  secrets directly.
- CSP is set in `tauri.conf.json`; capabilities grant only the plugins used.
- Telemetry is opt-in, content-free, and carries a random per-install id (rotated
  by "erase all data"). Crash reports are sent only on explicit user action.
