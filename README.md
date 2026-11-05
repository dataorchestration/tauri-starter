# Hello Desktop

A production-shaped starting point for **Tauri v2** desktop apps (macOS, Windows,
Linux). Everything a real app needs on day one is already wired, tested, and
configurable from one file — so you start on your product, not on plumbing.

| Included | Where |
| --- | --- |
| **Encrypted local database** — SQLCipher, key in the OS keychain, SQL migrations, CRUD for users / settings / notifications | `src-tauri/src/db/` |
| **Logging & telemetry** — `tracing` → rolling file log + swappable remote sinks (Sentry, generic HTTPS ingest). Opt-in, anonymous, no content | `src-tauri/src/telemetry/`, `docs/TELEMETRY.md` |
| **Crash reports** — panic hook writes a report; next launch offers send/dismiss. Frontend errors routed too | `src-tauri/src/crash.rs`, `src/lib/log.ts` |
| **Auto-updater** — signed feed on GitHub Releases, launch-time + manual check, "Restart to update" | `src-tauri/src/updater.rs`, `docs/RELEASING.md` |
| **Permissions health check** — status / request / deep-link to System Settings; required ones gate onboarding. Pure-Rust FFI on macOS | `src-tauri/src/permissions.rs`, `health.rs` |
| **Native menu bar + tray** — localized, platform-correct, rebuilt on language change | `src-tauri/src/menu.rs` |
| **i18n** — one JSON per language shared by Rust and React (`react-i18next`) | `src/i18n/`, `src-tauri/src/i18n.rs` |
| **UI foundation** — React 19, Tailwind v4 with light/dark design tokens, `zustand` store, typed IPC wrapper | `src/` |
| **Hello screen, Settings, Notifications, Permissions views** | `src/components/` |
| **Build, sign, notarize, publish** — per-platform scripts + CI matrix + tag-triggered release | `scripts/`, `.github/workflows/` |
| **Single instance, window-state restore, launch-at-login, native notifications** | feature flags |

## Quick start

```sh
npm install
npm run tauri dev
```

Prerequisites: Node 22+, Rust stable, and the [Tauri system deps](https://tauri.app/start/prerequisites/)
for your OS. Windows additionally needs `perl` + `nasm` (for SQLCipher's vendored OpenSSL):
`winget install StrawberryPerl.StrawberryPerl NASM.NASM`.

## Make it yours

```sh
npm run configure -- --name "Acme Notes" --identifier com.acme.notes \
  --org "Acme Inc" --homepage https://acme.com --repo acme/notes \
  --releases-repo acme/notes-releases --icon ./brand/icon.png
```

That rewrites the app name / bundle identifier / crate name everywhere, points the
updater at your releases repo, regenerates icons, and updates the CI workflow.
Run it again any time.

Feature toggles live in **`starter.config.json`** (read by both Rust and the
webview at build time):

```jsonc
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
  src/db/                encrypted store + migrations + CRUD
  src/telemetry/         Sink trait, tracing layer, sentry/http sinks
  src/crash.rs           panic hook + pending reports
  src/permissions.rs     per-platform permission status/request/settings links
  src/health.rs          aggregated health check
  src/menu.rs            app menu + tray
  src/updater.rs         update check/stage/notify
scripts/                 configure, release-{macos,windows,linux}, publish-feed
docs/                    RELEASING.md, ARCHITECTURE.md, TELEMETRY.md
```

## Verification gate

Everything must pass before a change is "done" (CI runs the same on all three OSes):

```sh
npm run check           # frontend tests + build, cargo fmt/clippy/test
npm run tauri build -- --bundles app
```

## Common extensions

- **New backend command** → add `#[tauri::command]` in `commands.rs`, register in
  `lib.rs`, add the typed entry in `src/lib/ipc.ts`.
- **New table** → `src-tauri/src/db/migrations/0002_*.sql`, list it in `db/mod.rs`,
  add a CRUD module like `users.rs`.
- **New telemetry backend** → implement `telemetry::Sink`, construct it in `telemetry::init`.
- **New language** → `src/i18n/locales/<code>.json`, import in `src/i18n/index.ts`
  and `src-tauri/src/i18n.rs`, add to `starter.config.json`. A test fails if any key is missing.
- **New permission** → extend `permissions::Permission` + the platform modules.

See `docs/ARCHITECTURE.md` for the data flow and `docs/RELEASING.md` for signing,
notarization and the update feed.
