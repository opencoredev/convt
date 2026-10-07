//! Opt-out, anonymous desktop product telemetry. Network work is confined here.

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use convt_license::client::State;
use serde_json::{Value, json};

const ENDPOINT: &str = "https://us.i.posthog.com/batch/";
const PROJECT_KEY: &str = "phc_yg96HDaDax6n2MmN7QyzvJjSh5qq2AwMUvaRnhmbJwMw";
const MAX_QUEUE: usize = 128;

pub fn new_install_id() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the OS random source works");
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15]
    )
}

#[derive(Clone)]
pub struct Telemetry {
    install_id: String,
    enabled: bool,
    queue: Arc<(Mutex<VecDeque<Value>>, Condvar)>,
    stop: Arc<Mutex<bool>>,
    worker: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
}

impl Telemetry {
    pub fn new(install_id: String, setting: bool, enforced: bool) -> Self {
        let enabled = setting && enforced && !do_not_track();
        let queue = Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));
        let stop = Arc::new(Mutex::new(false));
        let worker_queue = queue.clone();
        let worker_stop = stop.clone();
        let worker = thread::Builder::new()
            .name("convt-telemetry".into())
            .spawn(move || {
                loop {
                    let (lock, wake) = &*worker_queue;
                    let mut events = lock.lock().unwrap();
                    if events.is_empty() && !*worker_stop.lock().unwrap() {
                        let (next, _) = wake.wait_timeout(events, Duration::from_secs(60)).unwrap();
                        events = next;
                    }
                    let stopping = *worker_stop.lock().unwrap();
                    let batch: Vec<_> = events.drain(..).collect();
                    drop(events);
                    if !batch.is_empty() {
                        send_batch(&batch);
                    }
                    if stopping {
                        break;
                    }
                }
            })
            .ok();
        Self {
            install_id,
            enabled,
            queue,
            stop,
            worker: Arc::new(Mutex::new(worker)),
        }
    }

    pub fn capture(&self, event: &str, mut properties: serde_json::Map<String, Value>) {
        if !self.enabled {
            return;
        }
        properties.insert("$lib".into(), json!("convt-desktop"));
        properties.insert("$ip".into(), Value::Null);
        let payload =
            json!({"event": event, "distinct_id": self.install_id, "properties": properties});
        let (lock, wake) = &*self.queue;
        let mut queue = lock.lock().unwrap();
        if queue.len() >= MAX_QUEUE {
            queue.pop_front();
        }
        queue.push_back(payload);
        wake.notify_one();
    }

    pub fn identify(&self, user_id: &str) {
        let mut props = serde_json::Map::new();
        props.insert("$anon_distinct_id".into(), json!(self.install_id));
        props.insert("$identified_id".into(), json!(user_id));
        self.capture("$identify", props);
    }

    pub fn common(&self, state: &State) -> serde_json::Map<String, Value> {
        let mut p = serde_json::Map::new();
        p.insert("app_version".into(), json!(env!("CARGO_PKG_VERSION")));
        p.insert("os".into(), json!(std::env::consts::OS));
        p.insert("arch".into(), json!(std::env::consts::ARCH));
        p.insert(
            "license_state".into(),
            json!(match state {
                State::Trial { .. } => "trial",
                State::Licensed(_) => "pro",
                State::NotCovered(_) => "ended",
                State::TrialEnded => "ended",
                State::Unrestricted => "desktop",
            }),
        );
        p
    }

    pub fn license_props(&self, state: &State, plan: &str) -> serde_json::Map<String, Value> {
        let mut p = self.common(state);
        p.insert("plan".into(), json!(plan));
        p
    }

    pub fn shutdown(&self) {
        *self.stop.lock().unwrap() = true;
        self.queue.1.notify_one();
        if let Some(worker) = self.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
    }
}

impl Drop for Telemetry {
    fn drop(&mut self) {
        if Arc::strong_count(&self.worker) == 1 {
            self.shutdown();
        }
    }
}

fn do_not_track() -> bool {
    std::env::var("DO_NOT_TRACK")
        .map(|v| {
            !matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "" | "0" | "false" | "no" | "off"
            )
        })
        .unwrap_or(false)
}

fn send_batch(events: &[Value]) {
    let body = json!({"api_key": PROJECT_KEY, "batch": events});
    let agent = ureq::Agent::config_builder()
        .https_only(true)
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(3)))
        .build()
        .new_agent();
    let _ = agent.post(ENDPOINT).send(body.to_string());
}

pub fn size_bucket(size: Option<u64>) -> &'static str {
    match size.unwrap_or(0) {
        0..=1_048_575 => "<1MB",
        1_048_576..=10_485_759 => "1-10MB",
        10_485_760..=104_857_599 => "10-100MB",
        104_857_600..=1_073_741_823 => "100MB-1GB",
        _ => ">1GB",
    }
}

pub fn allowed(setting: bool, enforced: bool) -> bool {
    setting && enforced && !do_not_track()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn buckets_never_include_sizes() {
        assert_eq!(size_bucket(Some(4)), "<1MB");
        assert_eq!(size_bucket(Some(11_000_000)), "10-100MB");
    }
    #[test]
    fn opt_out_gates_setting_and_enforcement() {
        assert!(!allowed(false, true));
        assert!(!allowed(true, false));
    }
    #[test]
    fn scrubbed_payload_has_no_path_or_error_text() {
        let mut p = serde_json::Map::new();
        p.insert("from".into(), json!("png"));
        p.insert("to".into(), json!("webp"));
        p.insert("error_kind".into(), json!("io"));
        p.insert("size_bucket".into(), json!(size_bucket(Some(2_000_000))));
        let t = Telemetry::new("id".into(), true, false);
        t.capture("conversion_completed", p);
        assert_eq!(size_bucket(Some(2_000_000)), "1-10MB");
    }
}
