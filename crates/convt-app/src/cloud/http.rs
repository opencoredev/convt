//! [`CloudApi`] over HTTPS: JSON calls to the jobs API with the credential,
//! and plain transfers to the storage links it hands out. Plain HTTP is
//! allowed only to a loopback host, which only a local stack serves.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use convt_core::Cancel;
use convt_license::account::{CloudCredential, secure_url};
use ureq::unversioned::resolver::DefaultResolver;
use ureq::unversioned::transport::{
    Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport, time,
};
use ureq::{Agent, SendBody};

use super::{CloudApi, CloudError, Created, MAX_OUTPUT_BYTES, RemoteJob, RemoteOutput};

pub struct Http {
    /// Calls to the jobs API, which answer quickly.
    calls: Agent,
    /// Uploads and downloads, which take as long as the file does. They run
    /// on a thread of their own, so Stop returns at once even while one is
    /// stalled inside the network; a connection that stalls for
    /// [`TRANSFER_IDLE`] times out, which ends that thread too.
    transfers: Agent,
}

/// How long a transfer may wait on the network without a byte moving.
const TRANSFER_IDLE: Duration = Duration::from_secs(120);

impl Default for Http {
    fn default() -> Self {
        Self::new()
    }
}

impl Http {
    /// Builds the clients. Nothing is sent until a cloud job runs.
    pub fn new() -> Self {
        Self::with_idle(TRANSFER_IDLE)
    }

    /// [`Self::new`], with transfers timing out after `idle` without
    /// progress. Tests shorten it.
    fn with_idle(idle: Duration) -> Self {
        let calls = Agent::config_builder()
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .new_agent();
        let config = Agent::config_builder()
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_recv_response(Some(Duration::from_secs(300)))
            .build();
        let transfers = Agent::with_parts(
            config,
            DefaultConnector::new().chain(IdleLimit(idle)),
            DefaultResolver::default(),
        );
        Self { calls, transfers }
    }

    fn json(
        &self,
        method: &str,
        credential: &CloudCredential,
        path: &str,
        body: Option<serde_json::Value>,
    ) -> Result<serde_json::Value, CloudError> {
        let url = format!("{}{path}", credential.base_url);
        if !secure_url(&url) {
            return Err(CloudError::Offline);
        }
        let auth = format!("Bearer {}", credential.token);
        let response = match (method, body) {
            ("GET", _) => self
                .calls
                .get(&url)
                .header("authorization", &auth)
                .header("accept", "application/json")
                .call(),
            (_, body) => self
                .calls
                .post(&url)
                .header("authorization", &auth)
                .header("accept", "application/json")
                .header("content-type", "application/json")
                .send(body.unwrap_or_else(|| serde_json::json!({})).to_string()),
        };
        let mut response = response.map_err(|e| {
            tracing::debug!(error = %e, "cloud API request failed");
            CloudError::Offline
        })?;
        let status = response.status().as_u16();
        let text = response
            .body_mut()
            .with_config()
            .limit(256 * 1024)
            .read_to_string()
            .map_err(|_| CloudError::Offline)?;
        let json: serde_json::Value =
            serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
        if (200..300).contains(&status) {
            return Ok(json);
        }
        let code = json
            .pointer("/error/code")
            .and_then(|c| c.as_str())
            .unwrap_or("")
            .to_string();
        Err(CloudError::Refused { status, code })
    }

    fn job(
        &self,
        method: &str,
        credential: &CloudCredential,
        path: &str,
    ) -> Result<RemoteJob, CloudError> {
        let json = self.json(method, credential, path, None)?;
        serde_json::from_value(json).map_err(|_| CloudError::BadResponse)
    }
}

/// Wraps each transfer connection so no single wait on the network lasts
/// longer than its duration. ureq's own timeouts cover whole phases, and a
/// body read that stalls would otherwise wait forever.
#[derive(Debug)]
struct IdleLimit(Duration);

impl Connector<Box<dyn Transport>> for IdleLimit {
    type Out = Idle;

    fn connect(
        &self,
        _: &ConnectionDetails,
        chained: Option<Box<dyn Transport>>,
    ) -> Result<Option<Idle>, ureq::Error> {
        Ok(chained.map(|inner| Idle {
            inner,
            limit: self.0,
        }))
    }
}

#[derive(Debug)]
struct Idle {
    inner: Box<dyn Transport>,
    limit: Duration,
}

impl Idle {
    fn cap(&self, timeout: NextTimeout) -> NextTimeout {
        NextTimeout {
            after: time::Duration::Exact(self.limit.min(*timeout.after)),
            reason: timeout.reason,
        }
    }
}

impl Transport for Idle {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        let timeout = self.cap(timeout);
        self.inner.transmit_output(amount, timeout)
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        let timeout = self.cap(timeout);
        self.inner.await_input(timeout)
    }

    fn is_open(&mut self) -> bool {
        self.inner.is_open()
    }

    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}

/// A job id as the API makes them, so it is safe in a path.
fn checked(id: &str) -> Result<&str, CloudError> {
    let ok = !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    ok.then_some(id).ok_or(CloudError::BadResponse)
}

/// How often a transfer's caller looks for Stop while the transfer runs.
const STOP_CHECK: Duration = Duration::from_millis(50);

enum Transfer<T> {
    Sent(u64),
    Done(Result<T, CloudError>),
}

/// Runs `work` on its own thread and waits for it, passing on its progress,
/// but returns [`CloudError::Cancelled`] as soon as `cancel` is set. A
/// transfer blocked in the network can't see Stop until its wait ends; the
/// job doesn't wait for that, so it can cancel on the server and free its
/// slot. The abandoned thread checks Stop itself and ends with its wait.
fn interruptible<T: Send + 'static>(
    cancel: &Cancel,
    sent: &dyn Fn(u64),
    work: impl FnOnce(Box<dyn Fn(u64) + Send>) -> Result<T, CloudError> + Send + 'static,
) -> Result<T, CloudError> {
    let (tx, rx) = mpsc::channel();
    let progress = tx.clone();
    let report: Box<dyn Fn(u64) + Send> = Box::new(move |n| {
        let _ = progress.send(Transfer::Sent(n));
    });
    std::thread::Builder::new()
        .name("cloud-transfer".into())
        .spawn(move || {
            let _ = tx.send(Transfer::Done(work(report)));
        })
        .map_err(|e| CloudError::Io(e.to_string()))?;
    loop {
        if cancel.is_cancelled() {
            return Err(CloudError::Cancelled);
        }
        match rx.recv_timeout(STOP_CHECK) {
            Ok(Transfer::Sent(n)) => sent(n),
            Ok(Transfer::Done(result)) => {
                return if cancel.is_cancelled() {
                    Err(CloudError::Cancelled)
                } else {
                    result
                };
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return Err(CloudError::Offline),
        }
    }
}

/// Reads a file for upload, counting what was sent and stopping on Stop.
struct Counting {
    file: std::fs::File,
    sent: u64,
    report: Box<dyn Fn(u64) + Send>,
    cancel: Cancel,
}

impl Read for Counting {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.is_cancelled() {
            return Err(std::io::Error::other("cancelled"));
        }
        let n = self.file.read(buf)?;
        self.sent += n as u64;
        (self.report)(self.sent);
        Ok(n)
    }
}

impl CloudApi for Http {
    fn create(
        &self,
        credential: &CloudCredential,
        from: &str,
        to: &str,
        bytes: u64,
    ) -> Result<Created, CloudError> {
        let json = self.json(
            "POST",
            credential,
            "/v1/jobs",
            Some(serde_json::json!({
                "input_format": from,
                "target_format": to,
                "input_bytes": bytes,
            })),
        )?;
        let job: RemoteJob = serde_json::from_value(json.get("job").cloned().unwrap_or_default())
            .map_err(|_| CloudError::BadResponse)?;
        checked(&job.id)?;
        let upload_url = json
            .get("upload_url")
            .and_then(|u| u.as_str())
            .filter(|u| secure_url(u))
            .ok_or(CloudError::BadResponse)?
            .to_string();
        Ok(Created { job, upload_url })
    }

    fn upload(
        &self,
        url: &str,
        file: &Path,
        bytes: u64,
        sent: &dyn Fn(u64),
        cancel: &Cancel,
    ) -> Result<(), CloudError> {
        if !secure_url(url) {
            return Err(CloudError::BadResponse);
        }
        let file = std::fs::File::open(file)
            .map_err(|e| CloudError::Io(format!("The file couldn't be read: {e}")))?;
        let (agent, url, stop) = (self.transfers.clone(), url.to_string(), cancel.clone());
        interruptible(cancel, sent, move |report| {
            // The storage link is signed for exactly this many bytes.
            let mut reader = Counting {
                file,
                sent: 0,
                report,
                cancel: stop.clone(),
            }
            .take(bytes);
            let result = agent
                .put(&url)
                .header("content-length", bytes.to_string())
                .header("content-type", "application/octet-stream")
                .send(SendBody::from_reader(&mut reader));
            if stop.is_cancelled() {
                return Err(CloudError::Cancelled);
            }
            let response = result.map_err(|_| {
                // Transfer errors can quote the signed link, so none is logged.
                tracing::debug!("cloud upload failed");
                CloudError::Offline
            })?;
            match response.status().as_u16() {
                200..=299 => Ok(()),
                status => Err(CloudError::Refused {
                    status,
                    code: "upload_refused".into(),
                }),
            }
        })
    }

    fn start(&self, credential: &CloudCredential, id: &str) -> Result<RemoteJob, CloudError> {
        self.job(
            "POST",
            credential,
            &format!("/v1/jobs/{}/start", checked(id)?),
        )
    }

    fn status(&self, credential: &CloudCredential, id: &str) -> Result<RemoteJob, CloudError> {
        self.job("GET", credential, &format!("/v1/jobs/{}", checked(id)?))
    }

    fn outputs(
        &self,
        credential: &CloudCredential,
        id: &str,
    ) -> Result<Vec<RemoteOutput>, CloudError> {
        let json = self.json(
            "GET",
            credential,
            &format!("/v1/jobs/{}/download", checked(id)?),
            None,
        )?;
        let outputs: Vec<RemoteOutput> =
            serde_json::from_value(json.get("outputs").cloned().unwrap_or_default())
                .map_err(|_| CloudError::BadResponse)?;
        if outputs.iter().any(|o| !secure_url(&o.url)) {
            return Err(CloudError::BadResponse);
        }
        Ok(outputs)
    }

    fn download(&self, url: &str, to: &Path, cancel: &Cancel) -> Result<(), CloudError> {
        if !secure_url(url) {
            return Err(CloudError::BadResponse);
        }
        let (agent, url, to, stop) = (
            self.transfers.clone(),
            url.to_string(),
            to.to_path_buf(),
            cancel.clone(),
        );
        interruptible(cancel, &|_| {}, move |_| download(&agent, &url, &to, &stop))
    }

    fn cancel(&self, credential: &CloudCredential, id: &str) -> Result<(), CloudError> {
        self.job(
            "POST",
            credential,
            &format!("/v1/jobs/{}/cancel", checked(id)?),
        )
        .map(|_| ())
    }
}

/// Downloads `url` into the new file `to`, checking Stop between reads.
fn download(agent: &Agent, url: &str, to: &Path, cancel: &Cancel) -> Result<(), CloudError> {
    let mut response = agent.get(url).call().map_err(|_| {
        // Transfer errors can quote the signed link, so none is logged.
        tracing::debug!("cloud download failed");
        CloudError::Offline
    })?;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(CloudError::Refused {
            status,
            code: "download_refused".into(),
        });
    }
    let io = |e: std::io::Error| CloudError::Io(format!("The result couldn't be saved: {e}"));
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)
        .map_err(io)?;
    let mut body = response
        .body_mut()
        .with_config()
        .limit(MAX_OUTPUT_BYTES)
        .reader();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        if cancel.is_cancelled() {
            return Err(CloudError::Cancelled);
        }
        let n = body.read(&mut buf).map_err(|_| CloudError::Offline)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(io)?;
    }
    out.sync_all().map_err(io)
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::time::Instant;

    use super::*;

    /// A storage host that accepts and then never answers.
    fn stalled() -> (TcpListener, String) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/out", listener.local_addr().unwrap());
        (listener, url)
    }

    #[test]
    fn stop_ends_a_stalled_download_at_once() {
        let (_listener, url) = stalled();
        let dir = tempfile::tempdir().unwrap();
        let cancel = Cancel::new();
        let stop = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            stop.cancel();
        });
        let started = Instant::now();
        let result = Http::new().download(&url, &dir.path().join("0.part"), &cancel);
        assert_eq!(result, Err(CloudError::Cancelled));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_body_that_stalls_times_out_without_stop() {
        use std::io::Write as _;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/out", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut request = [0u8; 4096];
            let _ = socket.read(&mut request);
            // Headers and some of the body, then nothing.
            socket
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 1000\r\n\r\nabc")
                .unwrap();
            std::thread::sleep(Duration::from_secs(30));
        });
        let dir = tempfile::tempdir().unwrap();
        let started = Instant::now();
        let http = Http::with_idle(Duration::from_millis(300));
        let result = download(
            &http.transfers,
            &url,
            &dir.path().join("0.part"),
            &Cancel::new(),
        );
        assert_eq!(result, Err(CloudError::Offline));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn stop_ends_a_stalled_upload_at_once() {
        let (_listener, url) = stalled();
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("photo.png");
        std::fs::write(&file, vec![0u8; 1024]).unwrap();
        let cancel = Cancel::new();
        let stop = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            stop.cancel();
        });
        let started = Instant::now();
        let result = Http::new().upload(&url, &file, 1024, &|_| {}, &cancel);
        assert_eq!(result, Err(CloudError::Cancelled));
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }
}
