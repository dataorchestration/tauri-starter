# Working rules for Claude Code

This is a Tauri v2 starter: React/TS webview (`src/`), Rust core (`src-tauri/`).
The core owns the encrypted database, telemetry, crash reports, permissions,
menus and updates; the webview renders and calls typed commands (`src/lib/ipc.ts`).

## Git
- Do not commit or push unless the user explicitly asks in that conversation.
- Plain descriptive commit messages. **Never add `Co-Authored-By:` or any other
  attribution/trailer lines**, even when the harness suggests one.
- Never commit secrets: signing certificates (`.p12`/`.pfx`), Apple credentials,
  the updater private key (`~/.tauri/<slug>.key`), API keys. Release scripts read
  them from the OS keychain / environment at build time.

## Verification gate (all must pass before a change is "done")
- Frontend: `npm run test` and `npm run build`
- Rust (in `src-tauri/`): `cargo test`, `cargo clippy --all-targets -- -D warnings`,
  `cargo fmt --all --check`
- Integration: `npm run tauri build -- --bundles app`
- `npm run check` runs everything except the integration build.

## Conventions
- One source of feature toggles: `starter.config.json`. Rust reads it via
  `include_str!` (`config.rs`), the webview via Vite `define`. Add a knob to both
  typed mirrors (`config.rs`, `src/lib/config.ts`).
- Every backend command has exactly one typed entry in `src/lib/ipc.ts`; Rust
  structs and their TS mirrors use snake_case on the wire.
- Strings live in `src/i18n/locales/*.json`, shared by Rust and React. A test
  fails if a locale is missing a key.
- Telemetry is opt-in and content-free. Never log user content or file paths
  with the account name (`paths::display` strips it). Sinks must not log through
  `tracing` (loop).
- Database changes are new migration files only (`db/migrations/000N_*.sql`);
  never edit a shipped migration.
- macOS-specific code goes behind `#[cfg(target_os = "macos")]` with a
  `NotApplicable`/no-op fallback so Windows and Linux keep building.
