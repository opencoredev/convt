//! [`CloudApi`] over HTTPS: JSON calls to the jobs API with the credential,
//! and plain transfers to the storage links it hands out. Plain HTTP is
//! allowed only to a loopback host, which only a local stack serves.

use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

use convt_core::Cancel;
use convt_license::account::{CloudCredential, secure_url};
use ureq::{Agent, SendBody};

use super::{CloudApi, CloudError, Created, MAX_OUTPUT_BYTES, RemoteJob, RemoteOutput};

pub struct Http {
    /// Calls to the jobs API, which answer quickly.
    calls: Agent,
    /// Uploads and downloads, which take as long as the file does. Stop
    /// ends them; a connection that stalls times out.
    transfers: Agent,
}

impl Default for Http {
    fn default() -> Self {
        Self::new()
    }
}

impl Http {
    /// Builds the clients. Nothing is sent until a cloud job runs.
    pub fn new() -> Self {
        let calls = Agent::config_builder()
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(30)))
            .build()
            .new_agent();
        let transfers = Agent::config_builder()
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_recv_response(Some(Duration::from_secs(300)))
            .build()
            .new_agent();
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

/// A job id as the API makes them, so it is safe in a path.
fn checked(id: &str) -> Result<&str, CloudError> {
    let ok = !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    ok.then_some(id).ok_or(CloudError::BadResponse)
}

/// Reads a file for upload, counting what was sent and stopping on Stop.
struct Counting<'a> {
    file: std::fs::File,
    sent: u64,
    report: &'a dyn Fn(u64),
    cancel: &'a Cancel,
}

impl Read for Counting<'_> {
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
        // The storage link is signed for exactly this many bytes.
        let mut reader = Counting {
            file,
            sent: 0,
            report: sent,
            cancel,
        }
        .take(bytes);
        let result = self
            .transfers
            .put(url)
            .header("content-length", bytes.to_string())
            .header("content-type", "application/octet-stream")
            .send(SendBody::from_reader(&mut reader));
        if cancel.is_cancelled() {
            return Err(CloudError::Cancelled);
        }
        let response = result.map_err(|e| {
            tracing::debug!(error = %e, "cloud upload failed");
            CloudError::Offline
        })?;
        match response.status().as_u16() {
            200..=299 => Ok(()),
            status => Err(CloudError::Refused {
                status,
                code: "upload_refused".into(),
            }),
        }
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
        let mut response = self.transfers.get(url).call().map_err(|e| {
            tracing::debug!(error = %e, "cloud download failed");
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

    fn cancel(&self, credential: &CloudCredential, id: &str) -> Result<(), CloudError> {
        self.job(
            "POST",
            credential,
            &format!("/v1/jobs/{}/cancel", checked(id)?),
        )
        .map(|_| ())
    }
}
