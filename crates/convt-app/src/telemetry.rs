use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use convt_license::client::State as LicenseState;
use serde_json::{Value, json};

const CAPACITY: usize = 128;
const INTERVAL: Duration = Duration::from_secs(60);
const TIMEOUT: Duration = Duration::from_secs(2);
fn public_key() -> Option<String> {
    include_str!("../../../apps/web/wrangler.jsonc")
        .lines()
        .find_map(|line| {
            let value = line
                .trim()
                .strip_prefix("\"POSTHOG_KEY\":")?
                .trim()
                .trim_end_matches(',');
            serde_json::from_str(value).ok()
        })
}

pub fn do_not_track() -> bool {
    std::env::var("DO_NOT_TRACK").is_ok_and(|v| {
        !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "no" | "off"
        )
    })
}

pub fn install_id() -> Option<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).ok()?;
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Some(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}

pub fn valid_id(id: &str) -> bool {
    id.len() == 36
        && id.bytes().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
}

pub fn license_state(state: &LicenseState) -> &'static str {
    match state {
        LicenseState::Licensed(l) => match l.plan {
            convt_license::Plan::Desktop => "desktop",
            convt_license::Plan::Pro => "pro",
        },
        LicenseState::Trial { .. }
        | LicenseState::AccountTrial { .. }
        | LicenseState::Unrestricted => "trial",
        LicenseState::TrialEnded | LicenseState::SignInNeeded | LicenseState::NotCovered(_) => {
            "ended"
        }
    }
}

type Shared = Arc<(Mutex<State>, Condvar)>;

#[derive(Clone)]
pub struct Telemetry {
    shared: Shared,
    done: Arc<Mutex<std::sync::mpsc::Receiver<()>>>,
    _lifetime: Arc<Lifetime>,
}

struct Lifetime(Shared);

impl Drop for Lifetime {
    fn drop(&mut self) {
        let mut state = self.0.0.lock().unwrap();
        state.stop = true;
        state
            .shutdown_deadline
            .get_or_insert(Instant::now() + TIMEOUT);
        self.0.1.notify_one();
    }
}

struct State {
    enabled: bool,
    generation: u64,
    queue: VecDeque<Value>,
    stop: bool,
    shutdown_deadline: Option<Instant>,
    flush_at: Instant,
    install: String,
    user: Option<String>,
    identification: Option<Value>,
    #[cfg(test)]
    before_send: Option<(std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>)>,
}

pub enum Event {
    Opened,
    TrialStarted,
    TrialExpired,
    Upgrade(&'static str),
    Activated(convt_license::Plan),
}

#[derive(Clone)]
pub struct Conversion {
    telemetry: Telemetry,
    generation: u64,
    identity: String,
    license: &'static str,
}

impl Telemetry {
    pub fn new(install: String, enabled: bool) -> Self {
        #[cfg(not(test))]
        let endpoint = Some("https://us.i.posthog.com/batch/".to_string());
        #[cfg(test)]
        let endpoint = None;
        Self::start(install, enabled, endpoint, INTERVAL)
    }

    fn start(install: String, enabled: bool, endpoint: Option<String>, interval: Duration) -> Self {
        let shared = Arc::new((
            Mutex::new(State {
                enabled: enabled && !do_not_track(),
                generation: 0,
                queue: VecDeque::new(),
                stop: false,
                shutdown_deadline: None,
                flush_at: Instant::now() + interval,
                install,
                user: None,
                identification: None,
                #[cfg(test)]
                before_send: None,
            }),
            Condvar::new(),
        ));
        let (tx, rx) = std::sync::mpsc::channel();
        let this = Self {
            shared: shared.clone(),
            done: Arc::new(Mutex::new(rx)),
            _lifetime: Arc::new(Lifetime(shared.clone())),
        };
        std::thread::Builder::new()
            .name("convt-telemetry".into())
            .spawn(move || {
                let agent = ureq::Agent::config_builder()
                    .https_only(
                        endpoint
                            .as_ref()
                            .is_none_or(|url| url.starts_with("https://")),
                    )
                    .max_redirects(0)
                    .timeout_global(Some(TIMEOUT))
                    .build()
                    .new_agent();
                let (lock, wake) = &*shared;
                let key = public_key();
                loop {
                    let mut state = lock.lock().unwrap();
                    while !state.stop && Instant::now() < state.flush_at {
                        let delay = state.flush_at.saturating_duration_since(Instant::now());
                        state = wake.wait_timeout(state, delay).unwrap().0;
                    }
                    state.flush_at = Instant::now() + interval;
                    let stop = state.stop;
                    let generation = state.generation;
                    #[cfg(test)]
                    let before_send = state.before_send.take();
                    let batch: Vec<_> = if state.enabled && !do_not_track() {
                        state.queue.drain(..).collect()
                    } else {
                        state.queue.clear();
                        Vec::new()
                    };
                    drop(state);
                    if !batch.is_empty()
                        && let (Some(endpoint), Some(key)) = (&endpoint, &key)
                    {
                        let body = json!({"api_key": key, "batch": batch}).to_string();
                        #[cfg(test)]
                        if let Some((paused, resume)) = before_send {
                            let _ = paused.send(());
                            let _ = resume.recv_timeout(TIMEOUT);
                        }
                        let state = lock.lock().unwrap();
                        let timeout = state.shutdown_deadline.map_or(TIMEOUT, |deadline| {
                            deadline.saturating_duration_since(Instant::now())
                        });
                        let authorized = state.enabled
                            && state.generation == generation
                            && !do_not_track()
                            && !timeout.is_zero();
                        drop(state);
                        if authorized {
                            let _ = agent
                                .post(endpoint)
                                .header("content-type", "application/json")
                                .config()
                                .timeout_global(Some(timeout))
                                .build()
                                .send(body);
                        }
                    }
                    if stop {
                        break;
                    }
                }
                let _ = tx.send(());
            })
            .ok();
        this
    }

    pub fn set_enabled(&self, enabled: bool) {
        let mut state = self.shared.0.lock().unwrap();
        let enabled = enabled && !do_not_track();
        if state.enabled != enabled {
            state.generation += 1;
            state.queue.clear();
        }
        let turning_on = !state.enabled && enabled;
        state.enabled = enabled;
        if turning_on && let Some(identification) = state.identification.clone() {
            enqueue(&mut state, identification);
        }
    }

    pub fn reset_anonymous_id(&self, id: String) {
        let mut state = self.shared.0.lock().unwrap();
        state.enabled = false;
        state.generation += 1;
        state.queue.clear();
        state.install = id;
        state.user = None;
        state.identification = None;
    }

    pub fn identify(&self, user: Option<&str>, license: &LicenseState) {
        let mut state = self.shared.0.lock().unwrap();
        if state.user.as_deref() == user {
            return;
        }
        state.generation += 1;
        if state.user.is_some() {
            state.queue.clear();
        }
        state.user = user.map(str::to_string);
        state.identification = None;
        if let Some(user) = user {
            let payload = payload(
                "$identify",
                user,
                license_state(license),
                json!({"$anon_distinct_id": state.install}),
            );
            state.identification = Some(payload.clone());
            if state.queue.len() == CAPACITY {
                state.queue.pop_front();
            }
            enqueue(&mut state, payload);
        }
    }

    pub fn capture(&self, event: Event, license: &LicenseState) {
        let (name, properties) = match event {
            Event::Opened => ("app_opened", json!({})),
            Event::TrialStarted => ("trial_started", json!({})),
            Event::TrialExpired => ("trial_expired", json!({})),
            Event::Upgrade(source) => ("upgrade_clicked", json!({"source": source})),
            Event::Activated(plan) => (
                "license_activated",
                json!({"plan": match plan { convt_license::Plan::Desktop => "desktop", convt_license::Plan::Pro => "pro" }}),
            ),
        };
        let mut state = self.shared.0.lock().unwrap();
        let identity = state.user.as_ref().unwrap_or(&state.install);
        let event = payload(name, identity, license_state(license), properties);
        enqueue(&mut state, event);
    }

    pub fn conversion(&self, license: &LicenseState) -> Conversion {
        let state = self.shared.0.lock().unwrap();
        Conversion {
            telemetry: self.clone(),
            generation: state.generation,
            identity: state.user.as_ref().unwrap_or(&state.install).clone(),
            license: license_state(license),
        }
    }

    pub fn shutdown(&self) {
        {
            let mut state = self.shared.0.lock().unwrap();
            state.stop = true;
            state
                .shutdown_deadline
                .get_or_insert(Instant::now() + TIMEOUT);
            self.shared.1.notify_one();
        }
        let _ = self.done.lock().unwrap().recv_timeout(TIMEOUT);
    }

    #[cfg(test)]
    pub fn flush_now(&self) {
        self.shared.0.lock().unwrap().flush_at = Instant::now();
        self.shared.1.notify_one();
    }

    #[cfg(test)]
    pub fn local(install: String, enabled: bool, endpoint: String, interval: Duration) -> Self {
        assert!(endpoint.starts_with("http://127.0.0.1:"));
        Self::start(install, enabled, Some(endpoint), interval)
    }
}

fn enqueue(state: &mut State, payload: Value) {
    if state.enabled && !state.stop && !do_not_track() && state.queue.len() < CAPACITY {
        state.queue.push_back(payload);
    }
}

fn payload(name: &str, identity: &str, license: &str, mut properties: Value) -> Value {
    properties.as_object_mut().unwrap().extend(json!({"app_version": env!("CARGO_PKG_VERSION"), "os": std::env::consts::OS, "arch": std::env::consts::ARCH, "license_state": license, "$lib": "convt-desktop", "$ip": null, "$geoip_disable": true}).as_object().unwrap().clone());
    json!({"event": name, "distinct_id": identity, "properties": properties})
}

impl Conversion {
    pub fn started(&self, metadata: &Metadata) {
        self.capture("conversion_started", metadata.properties());
    }
    pub fn completed(&self, metadata: &Metadata, error: Option<&'static str>, elapsed: Duration) {
        let mut properties = metadata.properties();
        properties["success"] = json!(error.is_none());
        properties["error_kind"] = json!(error);
        properties["duration_ms"] = json!(elapsed.as_millis().min(u64::MAX as u128) as u64);
        self.capture("conversion_completed", properties);
    }
    fn capture(&self, name: &str, properties: Value) {
        let mut state = self.telemetry.shared.0.lock().unwrap();
        if state.generation == self.generation {
            enqueue(
                &mut state,
                payload(name, &self.identity, self.license, properties),
            );
        }
    }
}

pub struct Metadata {
    from: &'static str,
    to: &'static str,
    engines: Vec<&'static str>,
    size: &'static str,
}

impl Metadata {
    pub fn of(job: &convt_core::Job, registry: &convt_core::Registry, cloud: bool) -> Self {
        let from = convt_core::format_by_extension(&job.input);
        let engines = if cloud {
            vec!["cloud"]
        } else {
            from.and_then(|from| registry.plan(from, job.to).ok())
                .map(|plan| plan.hops.iter().map(|(engine, _)| engine.id()).collect())
                .unwrap_or_default()
        };
        let size = match std::fs::metadata(&job.input).ok().map(|m| m.len()) {
            None => "unknown",
            Some(0..=1_048_575) => "under_1mb",
            Some(1_048_576..=10_485_759) => "1_to_10mb",
            Some(10_485_760..=104_857_599) => "10_to_100mb",
            Some(_) => "100mb_plus",
        };
        Self {
            from: from.map_or("unknown", |f| f.id),
            to: job.to.id,
            engines,
            size,
        }
    }
    fn properties(&self) -> Value {
        json!({"from": self.from, "to": self.to, "engine": self.engines, "size_bucket": self.size})
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::{self, Receiver};

    pub fn sink(delay: Duration) -> (String, Receiver<(String, Value)>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}/batch/", listener.local_addr().unwrap());
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(90);
            while Instant::now() < deadline {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut head = String::new();
                let mut length = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 {
                        return;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        length = value.trim().parse().unwrap();
                    }
                    head.push_str(&line);
                    if line == "\r\n" {
                        break;
                    }
                }
                let mut body = vec![0; length];
                reader.read_exact(&mut body).unwrap();
                if tx
                    .send((head, serde_json::from_slice(&body).unwrap()))
                    .is_err()
                {
                    return;
                }
                std::thread::sleep(delay);
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
                );
            }
        });
        (url, rx)
    }

    #[test]
    fn actual_http_batches_allowlist_every_event_and_private_failure() {
        let (url, got) = sink(Duration::ZERO);
        let t = Telemetry::local(
            "00000000-0000-4000-8000-000000000001".into(),
            true,
            url,
            INTERVAL,
        );
        let license = LicenseState::Unrestricted;
        t.identify(Some("user_fake_55"), &license);
        for event in [
            Event::Opened,
            Event::TrialStarted,
            Event::TrialExpired,
            Event::Upgrade("settings"),
            Event::Activated(convt_license::Plan::Desktop),
            Event::Activated(convt_license::Plan::Pro),
        ] {
            t.capture(event, &license);
        }
        let job = convt_core::Job::new(
            "/private/secret-name.png",
            convt_core::format_by_id("webp").unwrap(),
        );
        let metadata = Metadata::of(&job, &convt_core::Registry::new(), false);
        let error = crate::jobs::JobError::from(&convt_core::Error::UndetectedFormat(
            "/private/error-name.token".into(),
        ));
        let handle = t.conversion(&license);
        handle.started(&metadata);
        handle.completed(&metadata, Some(error.kind), Duration::from_millis(42));
        t.shutdown();
        let (head, body) = got.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(head.starts_with("POST /batch/ "));
        assert!(
            head.to_ascii_lowercase()
                .contains("content-type: application/json")
        );
        let events = body["batch"].as_array().unwrap();
        println!("LOCAL_BATCH {}", serde_json::to_string(&body).unwrap());
        assert_eq!(events.len(), 9);
        let names: Vec<_> = events
            .iter()
            .map(|e| e["event"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "$identify",
                "app_opened",
                "trial_started",
                "trial_expired",
                "upgrade_clicked",
                "license_activated",
                "license_activated",
                "conversion_started",
                "conversion_completed"
            ]
        );
        for event in events {
            assert_eq!(event["distinct_id"], "user_fake_55");
            let properties = event["properties"].as_object().unwrap();
            for key in properties.keys() {
                assert!(
                    [
                        "app_version",
                        "os",
                        "arch",
                        "license_state",
                        "$lib",
                        "$ip",
                        "$geoip_disable",
                        "$anon_distinct_id",
                        "source",
                        "plan",
                        "from",
                        "to",
                        "engine",
                        "size_bucket",
                        "success",
                        "error_kind",
                        "duration_ms"
                    ]
                    .contains(&key.as_str()),
                    "{key}"
                );
            }
            assert_eq!(properties["$ip"], Value::Null);
            assert_eq!(properties["$geoip_disable"], true);
            assert_eq!(properties["$lib"], "convt-desktop");
        }
        for private in [
            "/private",
            "secret-name",
            "error-name",
            "token",
            "email",
            "message",
        ] {
            assert!(!body.to_string().contains(private), "{private}");
        }
        assert_eq!(events[8]["properties"]["duration_ms"], 42);
        assert_eq!(events[8]["properties"]["success"], false);
        println!(
            "LOCAL_CAPTURE events={names:?} distinct_id=user_fake_55 content_type=application/json path_and_error_text=absent"
        );
    }

    #[test]
    fn bounded_queue_opt_out_and_identity_generation_discard_stale_work() {
        let (url, got) = sink(Duration::ZERO);
        let t = Telemetry::local("installation".into(), true, url, INTERVAL);
        for _ in 0..CAPACITY + 40 {
            t.capture(Event::Opened, &LicenseState::Unrestricted);
        }
        assert_eq!(t.shared.0.lock().unwrap().queue.len(), CAPACITY);
        let handle = t.conversion(&LicenseState::Unrestricted);
        t.set_enabled(false);
        assert!(t.shared.0.lock().unwrap().queue.is_empty());
        t.set_enabled(true);
        handle.capture("conversion_completed", json!({}));
        assert!(t.shared.0.lock().unwrap().queue.is_empty());
        t.identify(Some("account_a"), &LicenseState::Unrestricted);
        let old = t.conversion(&LicenseState::Unrestricted);
        t.identify(None, &LicenseState::Unrestricted);
        t.identify(Some("account_b"), &LicenseState::Unrestricted);
        old.capture("conversion_completed", json!({}));
        t.set_enabled(false);
        t.shutdown();
        assert!(got.recv_timeout(Duration::from_millis(100)).is_err());
    }

    #[test]
    fn opt_out_discards_a_dequeued_batch_before_request_authorization() {
        let (url, got) = sink(Duration::ZERO);
        let t = Telemetry::local("installation".into(), true, url, Duration::from_millis(40));
        let (paused, reached) = mpsc::channel();
        let (resume, resumed) = mpsc::channel();
        t.shared.0.lock().unwrap().before_send = Some((paused, resumed));
        t.capture(Event::Opened, &LicenseState::Unrestricted);
        reached.recv_timeout(Duration::from_secs(2)).unwrap();
        t.set_enabled(false);
        t.set_enabled(true);
        resume.send(()).unwrap();
        t.shutdown();
        assert!(got.recv_timeout(Duration::from_millis(100)).is_err());
    }

    #[test]
    fn short_network_timeout_and_one_total_shutdown_deadline() {
        let (url, got) = sink(Duration::from_secs(4));
        let t = Telemetry::local("installation".into(), true, url, Duration::from_millis(10));
        t.capture(Event::Opened, &LicenseState::Unrestricted);
        got.recv_timeout(Duration::from_secs(2)).unwrap();
        let start = Instant::now();
        t.set_enabled(false);
        assert!(start.elapsed() < Duration::from_millis(100));
        t.set_enabled(true);
        t.capture(Event::Opened, &LicenseState::Unrestricted);
        t.shutdown();
        assert!(start.elapsed() < Duration::from_millis(2300));
        assert!(got.recv_timeout(Duration::from_millis(200)).is_err());
    }

    #[test]
    fn do_not_track_blocks_explicit_opt_in() {
        if std::env::var_os("CONVT_DNT_CHILD").is_some() {
            assert!(do_not_track());
            let (url, got) = sink(Duration::ZERO);
            let t = Telemetry::local("installation".into(), true, url, INTERVAL);
            t.set_enabled(true);
            t.capture(Event::Opened, &LicenseState::Unrestricted);
            t.shutdown();
            assert!(got.recv_timeout(Duration::from_millis(50)).is_err());
            return;
        }
        for value in ["1", "true", "yes", "on", "anything"] {
            assert!(
                std::process::Command::new(std::env::current_exe().unwrap())
                    .args([
                        "--exact",
                        "telemetry::tests::do_not_track_blocks_explicit_opt_in"
                    ])
                    .env("CONVT_DNT_CHILD", "1")
                    .env("DO_NOT_TRACK", value)
                    .status()
                    .unwrap()
                    .success()
            );
        }
    }

    #[test]
    fn install_ids_are_random_valid_uuids_and_tests_have_no_production_sink() {
        let a = install_id().unwrap();
        assert!(valid_id(&a));
        assert_ne!(a, install_id().unwrap());
        let t = Telemetry::new(a, true);
        t.capture(Event::Opened, &LicenseState::Unrestricted);
        t.shutdown();
        assert!(!crate::settings::Settings::default().telemetry);
    }

    #[test]
    fn source_defaults_send_nothing_and_events_wait_for_the_batch_interval() {
        let (url, got) = sink(Duration::ZERO);
        let t = Telemetry::local(
            "installation".into(),
            crate::settings::Settings::default().telemetry,
            url,
            INTERVAL,
        );
        t.capture(Event::Opened, &LicenseState::Unrestricted);
        t.shutdown();
        assert!(got.recv_timeout(Duration::from_millis(50)).is_err());
        let (url, got) = sink(Duration::ZERO);
        let t = Telemetry::local("installation".into(), true, url, Duration::from_millis(250));
        t.capture(Event::Opened, &LicenseState::Unrestricted);
        assert!(got.recv_timeout(Duration::from_millis(100)).is_err());
        got.recv_timeout(Duration::from_secs(2)).unwrap();
        t.shutdown();
    }

    #[test]
    fn offline_batches_are_dropped_without_retry_or_shutdown_delay() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/batch/", listener.local_addr().unwrap());
        drop(listener);
        let t = Telemetry::local(
            "installation".into(),
            true,
            endpoint,
            Duration::from_millis(10),
        );
        t.capture(Event::Opened, &LicenseState::Unrestricted);
        let start = Instant::now();
        t.shutdown();
        assert!(start.elapsed() < TIMEOUT);
        assert!(t.shared.0.lock().unwrap().queue.is_empty());
    }

    #[test]
    fn metadata_keeps_the_ordered_multihop_engine_sequence() {
        struct Hop(&'static str, &'static str, &'static str);
        impl convt_core::Engine for Hop {
            fn id(&self) -> &'static str {
                self.0
            }
            fn steps(&self) -> Vec<convt_core::Step> {
                vec![convt_core::Step {
                    from: convt_core::format_by_id(self.1).unwrap(),
                    to: convt_core::format_by_id(self.2).unwrap(),
                }]
            }
            fn convert(
                &self,
                _: &convt_core::Ctx,
                _: &std::path::Path,
                _: &std::path::Path,
            ) -> convt_core::Result<Vec<std::path::PathBuf>> {
                unreachable!()
            }
        }
        let mut registry = convt_core::Registry::new();
        registry.register(Arc::new(Hop("decode", "png", "jpeg")));
        registry.register(Arc::new(Hop("encode", "jpeg", "webp")));
        let metadata = Metadata::of(
            &convt_core::Job::new(
                "/private/input.png",
                convt_core::format_by_id("webp").unwrap(),
            ),
            &registry,
            false,
        );
        assert_eq!(metadata.properties()["engine"], json!(["decode", "encode"]));
        assert_eq!(metadata.properties()["from"], "png");
        assert_eq!(metadata.properties()["to"], "webp");
    }

    #[test]
    fn desktop_and_pro_licenses_have_distinct_common_states() {
        for (plan, expected) in [
            (convt_license::Plan::Desktop, "desktop"),
            (convt_license::Plan::Pro, "pro"),
        ] {
            let license = convt_license::License {
                id: "lic_fake".into(),
                email: "never-sent@convt.test".into(),
                plan,
                issued: "2026-01-01".into(),
                updates_until: "2027-01-01".into(),
            };
            assert_eq!(license_state(&LicenseState::Licensed(license)), expected);
        }
    }
}
