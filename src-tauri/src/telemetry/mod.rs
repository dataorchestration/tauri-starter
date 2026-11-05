//! Logging and telemetry with swappable backends.
//!
//! Everything funnels through the `tracing` facade (`tracing::info!` etc.):
//!
//! * a **file layer** writes rolling daily logs under the app log dir (always on
//!   unless disabled in config — it never leaves the device);
//! * a **stderr layer** in debug builds;
//! * a [`TelemetryLayer`] turns INFO/WARN/ERROR events into [`Event`]s and hands
//!   them to a background worker that batches them to every configured
//!   [`Sink`] (Sentry, a generic HTTPS ingest endpoint, …).
//!
//! Remote sinks are **opt-in**: while `Telemetry::is_enabled()` is false,
//! `emit` drops events before they are queued. Events carry an anonymous
//! per-install id and never content — add fields deliberately.
//!
//! Adding a backend = implement [`Sink`] (one `send(&[Event])`), construct it in
//! [`init`]. Sinks must NOT log through `tracing` (that would loop); use
//! `eprintln!`.

#[cfg(feature = "telemetry-http")]
pub mod http;
#[cfg(feature = "telemetry-sentry")]
pub mod sentry;

use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tracing::field::{Field, Visit};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::{Context, SubscriberExt};
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::TelemetryCfg;
use crate::paths::AppPaths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "error" => Self::Error,
            "warn" | "warning" => Self::Warn,
            "debug" | "trace" => Self::Debug,
            _ => Self::Info,
        }
    }
}

/// One telemetry record. The wire shape every sink receives.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub ts_ms: i64,
    pub level: Level,
    /// Category / `tracing` target, e.g. `"app"`, `"frontend"`, `"crash"`.
    pub name: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub fields: Map<String, Value>,
    pub app_version: String,
    pub os: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
}

/// Static facts attached to every event and crash report.
#[derive(Debug, Clone)]
pub struct Meta {
    pub app_name: String,
    pub app_version: String,
    pub identifier: String,
    pub os: String,
    pub arch: String,
    /// Random per-process id so events from one run can be grouped.
    pub session_id: String,
}

impl Meta {
    pub fn new(app_name: &str, app_version: &str, identifier: &str) -> Self {
        Self {
            app_name: app_name.into(),
            app_version: app_version.into(),
            identifier: identifier.into(),
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            session_id: uuid::Uuid::new_v4().to_string(),
        }
    }
}

/// A telemetry backend. Called from the worker thread with a non-empty batch.
pub trait Sink: Send + Sync {
    fn name(&self) -> &'static str;
    fn send(&self, batch: &[Event]);
    /// Block briefly until anything buffered inside the sink is out.
    fn flush(&self) {}
}

#[derive(Debug, Clone, Copy)]
pub struct BatchPolicy {
    pub max_events: usize,
    pub flush_interval: Duration,
}

impl Default for BatchPolicy {
    fn default() -> Self {
        Self {
            max_events: 20,
            flush_interval: Duration::from_secs(5),
        }
    }
}

enum Msg {
    Event(Event),
    Flush(SyncSender<()>),
}

struct Inner {
    tx: Mutex<Sender<Msg>>,
    enabled: AtomicBool,
    device_id: Mutex<Option<String>>,
    meta: Meta,
    sink_names: Vec<&'static str>,
}

/// Cheap to clone; all clones share one worker.
#[derive(Clone)]
pub struct Telemetry {
    inner: Arc<Inner>,
}

impl Telemetry {
    pub fn start(
        sinks: Vec<Box<dyn Sink>>,
        meta: Meta,
        enabled: bool,
        device_id: Option<String>,
        policy: BatchPolicy,
    ) -> Self {
        let sink_names = sinks.iter().map(|s| s.name()).collect();
        let (tx, rx) = mpsc::channel::<Msg>();
        std::thread::Builder::new()
            .name("telemetry".into())
            .spawn(move || worker(rx, sinks, policy))
            .expect("spawn telemetry worker");
        Self {
            inner: Arc::new(Inner {
                tx: Mutex::new(tx),
                enabled: AtomicBool::new(enabled),
                device_id: Mutex::new(device_id),
                meta,
                sink_names,
            }),
        }
    }

    /// No sinks at all (tests, tooling).
    pub fn disabled(meta: Meta) -> Self {
        Self::start(Vec::new(), meta, false, None, BatchPolicy::default())
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, on: bool) {
        self.inner.enabled.store(on, Ordering::Relaxed);
    }

    pub fn set_device_id(&self, id: Option<String>) {
        *self
            .inner
            .device_id
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = id;
    }

    pub fn meta(&self) -> &Meta {
        &self.inner.meta
    }

    pub fn sink_names(&self) -> &[&'static str] {
        &self.inner.sink_names
    }

    /// Queue an event for the remote sinks. Dropped when telemetry is off.
    pub fn emit(&self, level: Level, name: &str, message: &str, fields: Map<String, Value>) {
        if !self.is_enabled() || self.inner.sink_names.is_empty() {
            return;
        }
        let m = &self.inner.meta;
        let ev = Event {
            ts_ms: crate::db::now_millis(),
            level,
            name: name.into(),
            message: message.into(),
            fields,
            app_version: m.app_version.clone(),
            os: format!("{} {}", m.os, m.arch),
            session_id: m.session_id.clone(),
            device_id: self
                .inner
                .device_id
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone(),
        };
        let _ = self
            .inner
            .tx
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .send(Msg::Event(ev));
    }

    /// Push everything queued to the sinks and wait up to `timeout`.
    pub fn flush(&self, timeout: Duration) -> bool {
        let (ack_tx, ack_rx) = mpsc::sync_channel(1);
        if self
            .inner
            .tx
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .send(Msg::Flush(ack_tx))
            .is_err()
        {
            return false;
        }
        ack_rx.recv_timeout(timeout).is_ok()
    }
}

fn worker(rx: mpsc::Receiver<Msg>, sinks: Vec<Box<dyn Sink>>, policy: BatchPolicy) {
    let mut buf: Vec<Event> = Vec::new();
    let dispatch = |buf: &mut Vec<Event>| {
        if buf.is_empty() {
            return;
        }
        for s in &sinks {
            s.send(buf);
        }
        buf.clear();
    };
    loop {
        match rx.recv_timeout(policy.flush_interval) {
            Ok(Msg::Event(e)) => {
                buf.push(e);
                if buf.len() >= policy.max_events.max(1) {
                    dispatch(&mut buf);
                }
            }
            Ok(Msg::Flush(ack)) => {
                dispatch(&mut buf);
                for s in &sinks {
                    s.flush();
                }
                let _ = ack.send(());
            }
            Err(RecvTimeoutError::Timeout) => dispatch(&mut buf),
            Err(RecvTimeoutError::Disconnected) => {
                dispatch(&mut buf);
                break;
            }
        }
    }
}

/// `tracing` layer → [`Telemetry::emit`]. DEBUG/TRACE never leave the device.
pub struct TelemetryLayer {
    t: Telemetry,
}

impl TelemetryLayer {
    pub fn new(t: Telemetry) -> Self {
        Self { t }
    }
}

impl<S: tracing::Subscriber> Layer<S> for TelemetryLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _ctx: Context<'_, S>) {
        let level = match *event.metadata().level() {
            tracing::Level::ERROR => Level::Error,
            tracing::Level::WARN => Level::Warn,
            tracing::Level::INFO => Level::Info,
            _ => return,
        };
        let mut v = FieldVisitor::default();
        event.record(&mut v);
        self.t
            .emit(level, event.metadata().target(), &v.message, v.fields);
    }
}

#[derive(Default)]
struct FieldVisitor {
    message: String,
    fields: Map<String, Value>,
}

impl FieldVisitor {
    fn put(&mut self, field: &Field, v: Value) {
        if field.name() == "message" {
            self.message = match v {
                Value::String(s) => s,
                other => other.to_string(),
            };
        } else {
            self.fields.insert(field.name().to_string(), v);
        }
    }
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.put(field, Value::String(format!("{value:?}")));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.put(field, Value::String(value.into()));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.put(field, Value::from(value));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.put(field, Value::from(value));
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.put(field, Value::from(value));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.put(field, Value::from(value));
    }
}

/// Guards (non-blocking writer, Sentry client) that must live for the whole
/// process.
static GUARDS: OnceLock<Mutex<Vec<Box<dyn Any + Send + Sync>>>> = OnceLock::new();

fn keep(guard: Box<dyn Any + Send + Sync>) {
    GUARDS
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .push(guard);
}

/// Build the sinks from config, start the worker, install the global `tracing`
/// subscriber. Call once at startup, before anything logs.
pub fn init(
    paths: &AppPaths,
    cfg: &TelemetryCfg,
    meta: Meta,
    enabled: bool,
    device_id: String,
) -> Telemetry {
    #[allow(unused_mut)]
    let mut sinks: Vec<Box<dyn Sink>> = Vec::new();

    #[cfg(feature = "telemetry-sentry")]
    if !cfg.sinks.sentry.dsn.trim().is_empty()
        && let Some((sink, guard)) = sentry::init(&cfg.sinks.sentry, &meta)
    {
        sinks.push(Box::new(sink));
        keep(Box::new(guard));
    }
    #[cfg(feature = "telemetry-http")]
    if !cfg.sinks.http.endpoint.trim().is_empty() {
        sinks.push(Box::new(http::HttpSink::new(&cfg.sinks.http, &meta)));
    }

    let policy = BatchPolicy {
        max_events: cfg.sinks.http.batch_size.max(1),
        flush_interval: Duration::from_secs(cfg.sinks.http.flush_interval_secs.max(1)),
    };
    let telemetry = Telemetry::start(sinks, meta, enabled, Some(device_id), policy);

    let filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        tracing_subscriber::EnvFilter::new(if cfg!(debug_assertions) {
            "debug"
        } else {
            "info"
        })
    });

    let file_layer = if cfg.sinks.file.enabled {
        tracing_appender::rolling::RollingFileAppender::builder()
            .rotation(tracing_appender::rolling::Rotation::DAILY)
            .filename_prefix("app")
            .filename_suffix("log")
            .max_log_files(cfg.sinks.file.max_files.max(1))
            .build(&paths.logs)
            .map_err(|e| eprintln!("file logging disabled: {e}"))
            .ok()
            .map(|appender| {
                let (writer, guard) = tracing_appender::non_blocking(appender);
                keep(Box::new(guard));
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .with_target(true)
                    .with_writer(writer)
            })
    } else {
        None
    };
    let stderr_layer = cfg!(debug_assertions)
        .then(|| tracing_subscriber::fmt::layer().with_writer(std::io::stderr));

    // `try_init`: a second call (tests) must not panic.
    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .with(stderr_layer)
        .with(TelemetryLayer::new(telemetry.clone()))
        .try_init();

    telemetry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Capture(Mutex<Vec<Vec<Event>>>);
    impl Sink for Arc<Capture> {
        fn name(&self) -> &'static str {
            "capture"
        }
        fn send(&self, batch: &[Event]) {
            self.0.lock().unwrap().push(batch.to_vec());
        }
    }

    fn start(cap: &Arc<Capture>, enabled: bool, max: usize) -> Telemetry {
        Telemetry::start(
            vec![Box::new(cap.clone())],
            Meta::new("T", "0.0.1", "com.example.t"),
            enabled,
            Some("dev-1".into()),
            BatchPolicy {
                max_events: max,
                flush_interval: Duration::from_secs(60),
            },
        )
    }

    #[test]
    fn batches_by_size_and_on_flush() {
        let cap = Arc::new(Capture::default());
        let t = start(&cap, true, 2);
        for i in 0..3 {
            t.emit(Level::Info, "test", &format!("m{i}"), Map::new());
        }
        assert!(t.flush(Duration::from_secs(5)));
        let batches = cap.0.lock().unwrap();
        assert_eq!(
            batches.len(),
            2,
            "one full batch of 2, then the flushed remainder"
        );
        assert_eq!(batches[0].len(), 2);
        assert_eq!(batches[1].len(), 1);
        assert_eq!(batches[0][0].device_id.as_deref(), Some("dev-1"));
        assert_eq!(batches[0][0].app_version, "0.0.1");
    }

    #[test]
    fn disabled_sends_nothing_until_enabled() {
        let cap = Arc::new(Capture::default());
        let t = start(&cap, false, 1);
        t.emit(Level::Error, "test", "dropped", Map::new());
        assert!(t.flush(Duration::from_secs(5)));
        assert!(cap.0.lock().unwrap().is_empty());
        t.set_enabled(true);
        t.emit(Level::Error, "test", "kept", Map::new());
        assert!(t.flush(Duration::from_secs(5)));
        assert_eq!(cap.0.lock().unwrap().len(), 1);
    }

    #[test]
    fn tracing_layer_extracts_message_fields_and_skips_debug() {
        let cap = Arc::new(Capture::default());
        let t = start(&cap, true, 1);
        let subscriber = tracing_subscriber::registry().with(TelemetryLayer::new(t.clone()));
        tracing::subscriber::with_default(subscriber, || {
            tracing::warn!(target: "unit", code = 42, flag = true, "something {}", "happened");
            tracing::debug!(target: "unit", "never shipped");
        });
        assert!(t.flush(Duration::from_secs(5)));
        let batches = cap.0.lock().unwrap();
        assert_eq!(batches.len(), 1);
        let ev = &batches[0][0];
        assert_eq!(ev.level, Level::Warn);
        assert_eq!(ev.name, "unit");
        assert_eq!(ev.message, "something happened");
        assert_eq!(ev.fields["code"], 42);
        assert_eq!(ev.fields["flag"], true);
    }

    #[test]
    fn level_parses_loosely() {
        assert_eq!(Level::parse("WARNING"), Level::Warn);
        assert_eq!(Level::parse("trace"), Level::Debug);
        assert_eq!(Level::parse("whatever"), Level::Info);
    }
}
