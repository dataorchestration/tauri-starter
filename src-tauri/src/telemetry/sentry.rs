//! Sentry sink (cargo feature `telemetry-sentry`). Warnings and errors become
//! Sentry messages; info events become breadcrumbs on the next error. Panics
//! are captured by Sentry's own panic integration, which chains with the crash
//! reporter in `crash.rs`.

use std::time::Duration;

use super::{Event, Level, Meta, Sink};
use crate::config::SentrySinkCfg;

pub struct SentrySink;

/// Returns `None` when the DSN is invalid (Sentry disabled itself).
pub fn init(cfg: &SentrySinkCfg, meta: &Meta) -> Option<(SentrySink, sentry::ClientInitGuard)> {
    let mut opts = sentry::ClientOptions::default();
    opts.release = Some(format!("{}@{}", meta.identifier, meta.app_version).into());
    opts.send_default_pii = false;
    let guard = sentry::init((cfg.dsn.as_str(), opts));
    if !guard.is_enabled() {
        eprintln!("telemetry: sentry DSN rejected; sink disabled");
        return None;
    }
    sentry::configure_scope(|scope| {
        scope.set_tag("os", &meta.os);
        scope.set_tag("arch", &meta.arch);
        scope.set_tag("session", &meta.session_id);
    });
    Some((SentrySink, guard))
}

fn level(l: Level) -> sentry::Level {
    match l {
        Level::Error => sentry::Level::Error,
        Level::Warn => sentry::Level::Warning,
        Level::Info => sentry::Level::Info,
        Level::Debug => sentry::Level::Debug,
    }
}

impl Sink for SentrySink {
    fn name(&self) -> &'static str {
        "sentry"
    }

    fn send(&self, batch: &[Event]) {
        for e in batch {
            if e.level >= Level::Warn {
                sentry::with_scope(
                    |scope| {
                        for (k, v) in &e.fields {
                            scope.set_extra(k, v.clone());
                        }
                        scope.set_tag("category", &e.name);
                        if let Some(id) = &e.device_id {
                            scope.set_user(Some(sentry::User {
                                id: Some(id.clone()),
                                ..Default::default()
                            }));
                        }
                    },
                    || sentry::capture_message(&e.message, level(e.level)),
                );
            } else {
                sentry::add_breadcrumb(sentry::Breadcrumb {
                    category: Some(e.name.clone()),
                    message: Some(e.message.clone()),
                    level: level(e.level),
                    ..Default::default()
                });
            }
        }
    }

    fn flush(&self) {
        if let Some(client) = sentry::Hub::current().client() {
            client.flush(Some(Duration::from_secs(2)));
        }
    }
}
