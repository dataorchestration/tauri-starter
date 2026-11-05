# Logging & telemetry

Goal: you can debug a user's problem and see aggregate health **without** ever
receiving their content, and the user can see exactly what is sent and turn it
off (it is off by default).

## Layers

```
tracing::{info,warn,error}!(target: "…", field = value, "message")
   │
   ├─▶ file layer      ~/Library/Logs/<App>/app.YYYY-MM-DD.log  (always, local, 7 files)
   ├─▶ stderr layer    debug builds only
   └─▶ TelemetryLayer  INFO/WARN/ERROR → Event → worker thread → every Sink
                        (dropped at the door while telemetry is disabled)
```

The webview logs through `log.info/warn/error()` (`src/lib/log.ts`) which calls
the `log_event` command, so frontend logs land in the same file and sinks.
`window.onerror` / unhandled rejections go to `report_frontend_error`.

## The Event

```json
{
  "ts_ms": 1760000000000,
  "level": "warn",
  "name": "updater",
  "message": "launch update check failed",
  "fields": { "error": "timed out" },
  "app_version": "1.2.0",
  "os": "macos aarch64",
  "session_id": "uuid-per-process",
  "device_id": "uuid-per-install"
}
```

- `device_id` is random, generated on first run, stored in the encrypted DB, and
  rotated by "Erase all local data". It is not derived from hardware or accounts.
- `fields` is whatever the log call attached. **Do not log content**: no file
  contents, clipboard, window titles, user text. Paths go through
  `paths::display` (strips the home directory).

## Sinks

Implement `telemetry::Sink`:

```rust
pub trait Sink: Send + Sync {
    fn name(&self) -> &'static str;
    fn send(&self, batch: &[Event]);   // called on the worker thread, batch non-empty
    fn flush(&self) {}                 // best-effort drain, bounded time
}
```

Rules: never block for long, never buffer unboundedly, never log via `tracing`
from inside a sink (infinite loop) — use `eprintln!`.

### Built in

| Sink | Cargo feature | Config | Notes |
| --- | --- | --- | --- |
| file | — | `telemetry.sinks.file` | Not a `Sink`; it's the `tracing` file layer. Always local. |
| Sentry | `telemetry-sentry` (default on) | `telemetry.sinks.sentry.dsn` | WARN/ERROR → messages with `fields` as extras; INFO → breadcrumbs. Panics via Sentry's panic integration. `send_default_pii: false`. |
| HTTP ingest | `telemetry-http` (default on) | `telemetry.sinks.http.{endpoint,apiKeyHeader,apiKey,batchSize,flushIntervalSecs}` | `POST {source, events:[…]}` JSON. One retry on 5xx/network; 4xx drops. |

A sink with an empty DSN/endpoint is not constructed; `health_check` lists the
active ones (`telemetry_sinks`) so the Hello screen shows what's wired.

### Pointing the HTTP sink at your own backend

The generic sink is the seam for anything else — CloudWatch, PostHog, Loki,
BigQuery — via a tiny ingest endpoint you control (for example a Cloudflare
Worker). The app only ever holds a static API key for *your* endpoint, which
lets someone send you events and nothing else. **Never embed cloud-provider
credentials (AWS keys etc.) in a desktop binary**: they are trivially
extractable.

Minimal worker sketch:

```js
export default {
  async fetch(req, env) {
    if (req.method !== "POST" || req.headers.get("x-api-key") !== env.INGEST_KEY)
      return new Response(null, { status: 401 });
    const { source, events } = await req.json();
    // forward to CloudWatch Logs / PostHog / your DB here
    await env.EVENTS.put(`${source}/${Date.now()}`, JSON.stringify(events));
    return new Response(null, { status: 204 });
  },
};
```

## Opt-in and the UI

- `settings.telemetry_enabled` (default from `telemetry.defaultEnabled`, which
  should stay `false`). The Settings toggle calls `save_settings`, which flips
  `Telemetry::set_enabled` immediately.
- While disabled, `emit` returns before queueing — nothing is buffered to send
  later.
- Crash reports are the one exception: the user clicks **Send report**, which
  temporarily enables the pipeline for that single event and flushes
  synchronously (`send_crash`).

## Crash reports

`crash::install` sets a panic hook that writes
`crashes/crash-<ts>-<id>.json` with: message, `file:line:col`, thread name, Rust
backtrace, app version, OS. The hook then chains to the previous hook (Sentry's
if enabled) and the process aborts (`panic = "abort"` in release).

On next launch `pending_crashes` lists them; `CrashBanner` offers Send / Dismiss
(and shows the report). Debug builds have **Settings → Trigger a test crash** to
exercise the whole path.

## Local log files

`RUST_LOG` overrides the filter (`info` in release, `debug` in dev), e.g.
`RUST_LOG=tauri_starter_lib::telemetry=trace`. Settings → "Open logs folder"
opens the directory; the Help menu has the same entry.
