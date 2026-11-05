# Hello Desktop

A production-shaped starting point for **Tauri v2** desktop apps (macOS, Windows,
Linux). Everything a real app needs on day one is already wired, tested, and
configurable from one file — so you start on your product, not on plumbing.

| Included | Where |
| --- | --- |
| **Database choice** — SQLCipher (default) or native async Turso (MIT); encrypted storage, OS keychain, migrations, CRUD | `src-tauri/src/db/` |
| **Optional vector storage + search** — Turso collections, validated float32 embeddings, exact cosine / Euclidean search, typed IPC | `src-tauri/src/db/vectors.rs`, `docs/DATABASE.md` |
| **Logging & telemetry** — `tracing` → rolling file log + swappable remote sinks (Sentry, generic HTTPS ingest). Opt-in, anonymous, no content | `src-tauri/src/telemetry/`, `docs/TELEMETRY.md` |
| **Crash reports** — panic hook writes a report; next launch offers send/dismiss. Frontend errors routed too | `src-tauri/src/crash.rs`, `src/lib/log.ts` |
| **Auto-updater** — signed feed on GitHub Releases, launch-time + manual check, "Restart to update" | `src-tauri/src/updater.rs`, `docs/RELEASING.md` |
| **Permissions health check** — status / request / deep-link to System Settings; required ones gate onboarding. Pure-Rust FFI on macOS | `src-tauri/src/permissions.rs`, `health.rs` |
| **Native menu bar + tray** — localized, platform-correct, rebuilt on language change | `src-tauri/src/menu.rs` |
| **i18n** — one JSON per language shared by Rust and React (`react-i18next`) | `src/i18n/`, `src-tauri/src/i18n.rs` |
| **UI foundation** — React 19, Tailwind v4 with light/dark design tokens, `zustand` store, typed IPC wrapper | `src/` |
| **Hello screen, Settings, Notifications, Permissions views** | `src/components/` |
| **Build, sign, notarize, publish** — per-platform scripts + CI matrix + manual bundle build + tag-triggered release | `scripts/`, `.github/workflows/` |
| **Single instance, window-state restore, launch-at-login, native notifications** | feature flags |

## Quick start

```sh
npm install
npm run tauri dev
```

Prerequisites: Node 22+, Rust stable, and the [Tauri system deps](https://tauri.app/start/prerequisites/)
for your OS. The current locked frontend dependencies need Node 22.22.2+ within the 22.x line.
Windows SQLCipher builds (and Turso builds with legacy import) additionally need
`perl` + `nasm` for vendored OpenSSL:
`winget install StrawberryPerl.StrawberryPerl NASM.NASM`.
Fresh Turso builds exclude SQLCipher and these OpenSSL build requirements.

## Choose your database

After **Use this template**, run `npm run configure` in a terminal. Setup prompts
for the app identity and database choice. Or select explicitly:

```sh
# Default: encrypted SQLCipher, no vector API
npm run configure -- --database sqlcipher

# Native async Turso + exact vector search, no SQLCipher dependency
npm run configure -- --database turso --import-sqlcipher false

# Upgrade an existing stock SQLCipher install to Turso
npm run configure -- --database turso --import-sqlcipher true
```

| Pick | Includes | Consideration |
| --- | --- | --- |
| SQLCipher (default) | Encrypted CRUD for users, settings, notifications | Synchronous engine runs on blocking workers; vectors unavailable |
| Turso | Native async/await, encrypted CRUD, exact vector search | Pinned 0.8.3; encryption is experimental upstream; no ANN index |
| Turso + SQLCipher import | Turso plus one-time encrypted legacy import | Retains SQLCipher dependency and the original data snapshot |

Setup saves the choice in `starter.config.json` and generates Cargo defaults.
A build guard rejects mismatches. Only the selected engine (plus an explicitly
selected importer) is compiled. Database-only commands preserve the app identity.
This is a **project setup choice**, not an end-user runtime switch.
See [Database guide](docs/DATABASE.md) before changing an existing app.

## Make it yours

```sh
npm run configure -- --name "Acme Notes" --identifier com.acme.notes \
  --org "Acme Inc" --homepage https://acme.com --repo acme/notes \
  --database sqlcipher \
  --releases-repo acme/notes-releases --icon ./brand/icon.png
```

That rewrites the app name / bundle identifier / crate name everywhere, points the
updater at your releases repo, regenerates icons, and updates the CI workflow.
Run it again any time.

Feature toggles live in **`starter.config.json`** (read by both Rust and the
webview at build time):

```jsonc
"database":    { "engine": "sqlcipher", "importSqlcipher": false },
"features":    { "tray": true, "singleInstance": true, "windowState": true,
                 "autostart": true, "updater": false, "notifications": true },
"permissions": { "required": [], "optional": ["notifications"] },
"telemetry":   { "defaultEnabled": false, "sinks": { "file": {…}, "sentry": { "dsn": "" }, "http": { "endpoint": "" } } },
"i18n":        { "default": "en", "supported": ["en", "es"] }
```

Heavy optional dependencies are also Cargo features (`telemetry-sentry`,
`telemetry-http`, both on by default) so you can strip what you don't ship.

## Layout

```
starter.config.json      feature toggles (single source for Rust + frontend)
src/                     React webview
  components/            Shell, Hello, Settings, Notifications, Permissions, banners
  lib/ipc.ts             typed `invoke` wrapper — one entry per backend command
  lib/events.ts          typed core→UI events
  lib/log.ts             frontend logging + global error handlers
  store/app.ts           zustand store
  i18n/locales/*.json    translations (shared with Rust)
  index.css              Tailwind v4 + design tokens (light/dark)
src-tauri/
  src/lib.rs             startup sequence, plugin wiring
  src/commands.rs        the IPC surface
  src/config.rs          starter.config.json → typed struct
  src/db/                selected engine + shared models/tests + optional vectors/import
  src/telemetry/         Sink trait, tracing layer, sentry/http sinks
  src/crash.rs           panic hook + pending reports
  src/permissions.rs     per-platform permission status/request/settings links
  src/health.rs          aggregated health check
  src/menu.rs            app menu + tray
  src/updater.rs         update check/stage/notify
scripts/                 configure, release-{macos,windows,linux}, publish-feed
docs/                    DATABASE.md, RELEASING.md, ARCHITECTURE.md, TELEMETRY.md
```

## Verification gate

Everything must pass before a change is "done" — run it locally. Push/PR CI is
kept cheap for the GitHub free plan (one Linux runner: frontend tests + build,
`cargo fmt --check`; nothing compiles Rust). Clippy + tests and app bundles are
manual workflows (`rust-checks.yml`, `build.yml`):

```sh
npm run check           # frontend tests + build, cargo fmt/clippy/test
npm run tauri build -- --bundles app
```

## Common extensions

- **New backend command** → add `#[tauri::command]` in `commands.rs`, register in
  `lib.rs`, add the typed entry in `src/lib/ipc.ts`.
- **New table** → `src-tauri/src/db/migrations/0003_*.sql`, list it in the relevant
  backend store module, and add shared models plus backend CRUD like `users.rs`.
- **New telemetry backend** → implement `telemetry::Sink`, construct it in `telemetry::init`.
- **New language** → `src/i18n/locales/<code>.json`, import in `src/i18n/index.ts`
  and `src-tauri/src/i18n.rs`, add to `starter.config.json`. A test fails if any key is missing.
- **New permission** → extend `permissions::Permission` + the platform modules.

See `docs/ARCHITECTURE.md` for the data flow and `docs/RELEASING.md` for signing,
notarization and the update feed.
