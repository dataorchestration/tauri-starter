//! Generic HTTPS ingest sink (cargo feature `telemetry-http`).
//!
//! POSTs `{"source": <identifier>, "events": [Event, …]}` as JSON. Point it at
//! anything that accepts that — a Cloudflare Worker that forwards to
//! CloudWatch, a PostHog/Loki adapter, your own API. No cloud credentials ever
//! ship in the binary: at most a static API key for your own endpoint, which
//! only lets someone *send* you events.

use std::time::Duration;

use super::{Event, Meta, Sink};
use crate::config::HttpSinkCfg;

pub struct HttpSink {
    client: reqwest::blocking::Client,
    endpoint: String,
    api_key: Option<(String, String)>,
    source: String,
}

impl HttpSink {
    pub fn new(cfg: &HttpSinkCfg, meta: &Meta) -> Self {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent(format!("{}/{}", meta.identifier, meta.app_version))
            .build()
            .expect("reqwest client");
        let api_key =
            (!cfg.api_key.is_empty()).then(|| (cfg.api_key_header.clone(), cfg.api_key.clone()));
        Self {
            client,
            endpoint: cfg.endpoint.clone(),
            api_key,
            source: meta.identifier.clone(),
        }
    }

    fn post(&self, body: &serde_json::Value) -> Result<u16, String> {
        let mut req = self.client.post(&self.endpoint).json(body);
        if let Some((h, v)) = &self.api_key {
            req = req.header(h.as_str(), v.as_str());
        }
        req.send()
            .map(|r| r.status().as_u16())
            .map_err(|e| e.to_string())
    }
}

impl Sink for HttpSink {
    fn name(&self) -> &'static str {
        "http"
    }

    /// One retry on network error / 5xx; 4xx is our bug, so no retry. Failures
    /// drop the batch — telemetry must never block or buffer unbounded.
    fn send(&self, batch: &[Event]) {
        let body = serde_json::json!({ "source": self.source, "events": batch });
        for attempt in 0..2 {
            match self.post(&body) {
                Ok(s) if (200..300).contains(&s) => return,
                Ok(s) if (400..500).contains(&s) => {
                    eprintln!(
                        "telemetry: ingest rejected batch ({s}); dropping {} events",
                        batch.len()
                    );
                    return;
                }
                Ok(s) => eprintln!("telemetry: ingest returned {s} (attempt {})", attempt + 1),
                Err(e) => eprintln!(
                    "telemetry: ingest unreachable: {e} (attempt {})",
                    attempt + 1
                ),
            }
            if attempt == 0 {
                std::thread::sleep(Duration::from_millis(500));
            }
        }
        eprintln!("telemetry: dropped {} events", batch.len());
    }
}
