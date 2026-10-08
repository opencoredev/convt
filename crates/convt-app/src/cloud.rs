//! Converting on convt's cloud instead of on this computer. Only a signed-in
//! account with paid Pro can, and only after the user agrees to upload the
//! file, since the cloud is the one place a file leaves the machine.
//!
//! Nothing here touches the network until the user starts a cloud
//! conversion: [`access`] decides from what is already on this computer, and
//! [`Http`] only builds its clients. A cloud job then asks convt.app for a
//! five-minute credential with the device token (convt.app checks Pro again),
//! and runs against the cloud jobs API in `crates/convt-server/openapi.json`:
//! create, upload, start, poll, download, and cancel on Stop. The results are
//! published by [`Destination`], so they land where a local conversion's
//! would, under the same names.
//!
//! Logs carry job ids and error codes, never file names, URLs or
//! credentials.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use convt_core::{Cancel, Destination, Format, Job, format_by_extension};
use convt_license::account::{self, ApiError, CloudCredential};
use convt_license::{Plan, client::State, date};
use serde::Deserialize;

use crate::jobs::{JobError, JobResult};

mod http;
#[cfg(test)]
mod tests;

pub use http::Http;

/// Whether the Cloud choice in Quick convert can be used, and if not, why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudAccess {
    /// Signed in with paid Pro; cloud conversions can run.
    Ready,
    /// Not signed in to convt.app on this computer.
    SignedOut,
    /// Signed in, but the account has no paid Pro.
    NeedsPro,
    /// Something else stops it, in words for the user (offline, not set up
    /// in this build).
    Unavailable(String),
}

impl CloudAccess {
    // Quick convert's Cloud choice asks this.
    #[allow(dead_code)]
    pub fn ready(&self) -> bool {
        matches!(self, CloudAccess::Ready)
    }

    /// What the disabled Cloud choice says on hover.
    pub fn reason(&self) -> Option<String> {
        match self {
            CloudAccess::Ready => None,
            CloudAccess::SignedOut => {
                Some("Sign in with a Pro account to convert in the cloud.".into())
            }
            CloudAccess::NeedsPro => Some("Cloud conversion is part of Pro.".into()),
            CloudAccess::Unavailable(why) => Some(why.clone()),
        }
    }
}

/// Where cloud conversion stands, from what this computer already knows:
/// whether this build has a site to sign in to, a stored sign-in, and the
/// stored license. Pro must be current today (UTC days since the epoch).
/// Builds from source check no license, so a sign-in is enough there;
/// convt.app checks Pro either way before it issues a credential.
pub fn access(account_url: &str, signed_in: bool, license: &State, today: i64) -> CloudAccess {
    if account_url.is_empty() {
        return CloudAccess::Unavailable("Cloud conversion isn't set up in this build.".into());
    }
    if !signed_in {
        return CloudAccess::SignedOut;
    }
    match license {
        State::Unrestricted => CloudAccess::Ready,
        State::Licensed(l)
            if l.plan == Plan::Pro
                && date::to_days(&l.updates_until).is_some_and(|until| until >= today) =>
        {
            CloudAccess::Ready
        }
        _ => CloudAccess::NeedsPro,
    }
}

/// The largest input the cloud takes (convt-server's `MAX_FILE_BYTES`).
pub const MAX_INPUT_BYTES: u64 = 2_000_000_000;
/// The largest output the app downloads. The worker keeps each output
/// under 2 GB and all of a job's under 4 GB.
const MAX_OUTPUT_BYTES: u64 = 4_000_000_000;
/// How long a credential is used before a fresh one is asked for. They last
/// five minutes.
const CREDENTIAL_REUSE: Duration = Duration::from_secs(240);
/// How long the app keeps polling through network errors before it gives up.
const OFFLINE_GRACE: Duration = Duration::from_secs(120);

/// Whether convt's cloud converts `from` to `to`, by the list the cloud's
/// workers were built with.
pub fn supports(from: &Format, to: &Format) -> bool {
    #[derive(Deserialize)]
    struct Capabilities {
        formats: Vec<Entry>,
    }
    #[derive(Deserialize)]
    struct Entry {
        id: String,
        targets: Vec<String>,
    }
    static LIST: std::sync::OnceLock<Vec<Entry>> = std::sync::OnceLock::new();
    let list = LIST.get_or_init(|| {
        serde_json::from_str::<Capabilities>(include_str!("../../convt-server/cloud-formats.json"))
            .map(|c| c.formats)
            .unwrap_or_default()
    });
    list.iter()
        .any(|e| e.id == from.id && e.targets.iter().any(|t| t == to.id))
}

/// A cloud job's state, as convt-server reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RemoteStatus {
    Created,
    Uploaded,
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RemoteJob {
    pub id: String,
    pub status: RemoteStatus,
    #[serde(default)]
    pub error_code: Option<String>,
}

/// A new job and where to upload its input.
#[derive(Clone, PartialEq, Eq)]
pub struct Created {
    pub job: RemoteJob,
    pub upload_url: String,
}

/// A finished job's output and a short-lived link to it.
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct RemoteOutput {
    pub name: String,
    pub url: String,
}

/// Why a call to the cloud API failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudError {
    /// No answer: no connection, or it broke off.
    Offline,
    /// The API answered with an error status and code (`limit_reached`).
    Refused { status: u16, code: String },
    /// An answer this build doesn't understand.
    BadResponse,
    /// The user stopped the job while a transfer ran.
    Cancelled,
    /// A file on this computer couldn't be read or written.
    Io(String),
}

/// The cloud jobs API. Tests script their own.
pub trait CloudApi: Send + Sync {
    fn create(
        &self,
        credential: &CloudCredential,
        from: &str,
        to: &str,
        bytes: u64,
    ) -> Result<Created, CloudError>;
    /// Uploads exactly `bytes` bytes of `file`. `sent` gets the bytes sent
    /// so far.
    fn upload(
        &self,
        url: &str,
        file: &Path,
        bytes: u64,
        sent: &dyn Fn(u64),
        cancel: &Cancel,
    ) -> Result<(), CloudError>;
    fn start(&self, credential: &CloudCredential, id: &str) -> Result<RemoteJob, CloudError>;
    fn status(&self, credential: &CloudCredential, id: &str) -> Result<RemoteJob, CloudError>;
    fn outputs(
        &self,
        credential: &CloudCredential,
        id: &str,
    ) -> Result<Vec<RemoteOutput>, CloudError>;
    /// Downloads `url` into the new file `to`.
    fn download(&self, url: &str, to: &Path, cancel: &Cancel) -> Result<(), CloudError>;
    fn cancel(&self, credential: &CloudCredential, id: &str) -> Result<(), CloudError>;
}

/// Gets and reuses credentials for one batch of cloud jobs.
pub struct Credentials {
    api: Arc<dyn account::Api>,
    device_token: String,
    cached: Mutex<Option<(CloudCredential, Instant)>>,
}

impl Credentials {
    pub fn new(api: Arc<dyn account::Api>, device_token: String) -> Self {
        Self {
            api,
            device_token,
            cached: Mutex::new(None),
        }
    }

    /// A credential that has a few minutes left. `fresh` asks for a new one
    /// even if the last one should still work.
    fn get(&self, fresh: bool) -> Result<CloudCredential, ApiError> {
        let mut cached = self.cached.lock().unwrap();
        if !fresh
            && let Some((credential, at)) = &*cached
            && at.elapsed() < CREDENTIAL_REUSE
        {
            return Ok(credential.clone());
        }
        let credential = self.api.cloud_credential(&self.device_token)?;
        *cached = Some((credential.clone(), Instant::now()));
        Ok(credential)
    }
}

/// What a cloud job needs: the API, credentials for it, and how often to
/// ask how the job is doing.
pub struct Cloud {
    pub api: Arc<dyn CloudApi>,
    pub credentials: Credentials,
    pub poll: Duration,
}

fn fail(kind: &'static str, message: impl Into<String>) -> JobError {
    JobError {
        kind,
        message: message.into(),
    }
}

fn cancelled() -> JobError {
    fail("cancelled", "Cancelled.")
}

/// A refused credential, in words for the user.
fn credential_error(e: ApiError) -> JobError {
    match e {
        ApiError::NeedsPro => fail("cloud_pro", e.to_string()),
        ApiError::SignedOut => fail(
            "cloud_signed_out",
            "This computer was signed out of convt.app. Sign in again to convert in the cloud.",
        ),
        ApiError::Offline => fail("cloud_offline", e.to_string()),
        ApiError::RateLimited => fail(
            "cloud_busy",
            "Too many cloud conversions at once. Wait a few minutes, then try again.",
        ),
        e => fail("cloud_unavailable", e.to_string()),
    }
}

/// An API failure, in words for the user.
fn api_error(e: CloudError, to: &Format) -> JobError {
    let target = to.extension().to_uppercase();
    match e {
        CloudError::Cancelled => cancelled(),
        CloudError::Offline => fail(
            "cloud_offline",
            "convt's cloud couldn't be reached. Check your internet connection and try again.",
        ),
        CloudError::Io(message) => fail("io", message),
        CloudError::BadResponse => fail(
            "cloud_server",
            "convt's cloud sent an answer this version of convt doesn't understand.",
        ),
        CloudError::Refused { status, code } => match code.as_str() {
            "file_too_large" => fail("cloud_too_large", "The cloud takes files up to 2 GB."),
            "unsupported_format" => fail(
                "cloud_unsupported",
                format!("convt's cloud can't convert this file to {target}."),
            ),
            "limit_reached" => fail(
                "cloud_limit",
                "You've used this month's 50 GB of cloud conversion. It resets on the 1st (UTC).",
            ),
            "storage_limit_reached" => fail(
                "cloud_limit",
                "Too many recent cloud jobs are still stored. Try again once they're \
                 cleaned up, within 24 hours.",
            ),
            "not_enrolled" => fail(
                "cloud_pro",
                "Cloud conversion needs an active paid Pro subscription.",
            ),
            "unauthorized" => fail(
                "cloud_signed_out",
                "convt's cloud didn't accept this computer's sign-in. Try again.",
            ),
            "rate_limited" => fail(
                "cloud_busy",
                "Too many cloud requests. Wait a minute, then try again.",
            ),
            "size_mismatch" => fail(
                "cloud_upload",
                "The file changed while it was uploading. Try again.",
            ),
            _ => fail(
                "cloud_server",
                format!("convt's cloud had a problem (HTTP {status}). Try again later."),
            ),
        },
    }
}

/// A failed job's code, in words for the user.
fn remote_failure(code: Option<&str>) -> JobError {
    match code {
        Some("expired") => fail(
            "cloud_expired",
            "The cloud job expired before it finished. Try again.",
        ),
        Some("worker_shutdown") => fail(
            "cloud_server",
            "convt's cloud stopped during the conversion. Try again.",
        ),
        _ => fail("cloud_failed", "convt's cloud couldn't convert this file."),
    }
}

/// Whether an error is worth asking again for, after a pause.
fn transient(e: &CloudError) -> bool {
    match e {
        CloudError::Offline => true,
        CloudError::Refused { status, code } => {
            *status == 429 || *status >= 500 || code == "storage_unavailable"
        }
        _ => false,
    }
}

fn unauthorized(e: &CloudError) -> bool {
    matches!(e, CloudError::Refused { code, .. } if code == "unauthorized")
}

/// Waits `d`, or less if the job is cancelled. True if it was.
fn sleep(d: Duration, cancel: &Cancel) -> bool {
    let until = Instant::now() + d;
    while Instant::now() < until {
        if cancel.is_cancelled() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50).min(until - Instant::now()));
    }
    cancel.is_cancelled()
}

/// Runs one job on convt's cloud and publishes its results like a local
/// conversion. Progress: the upload is the first half; while the cloud
/// converts there is none to report; downloading finishes it.
pub fn run(cloud: &Cloud, job: &Job, progress: &dyn Fn(Option<f32>), cancel: &Cancel) -> JobResult {
    let to = job.to;
    let from = format_by_extension(&job.input).ok_or_else(|| {
        fail(
            "unsupported",
            "convt can't tell what kind of file this is from its name.",
        )
    })?;
    if !supports(from, to) {
        return Err(fail(
            "cloud_unsupported",
            format!(
                "convt's cloud can't convert {} to {}.",
                from.extension().to_uppercase(),
                to.extension().to_uppercase()
            ),
        ));
    }
    let bytes = std::fs::metadata(&job.input)
        .map_err(|e| fail("io", format!("The file couldn't be read: {e}")))?
        .len();
    if bytes == 0 {
        return Err(fail("cloud_empty", "The file is empty."));
    }
    if bytes > MAX_INPUT_BYTES {
        return Err(fail("cloud_too_large", "The cloud takes files up to 2 GB."));
    }
    // Made before anything is uploaded, so a folder that can't be written
    // fails here and not after the conversion.
    let destination = Destination::of(job).map_err(|e| JobError::from(&e))?;
    let staging = destination.staging().map_err(|e| JobError::from(&e))?;
    let err = |e| api_error(e, to);

    let mut session = Session {
        cloud,
        credential: cloud.credentials.get(false).map_err(credential_error)?,
    };
    if cancel.is_cancelled() {
        return Err(cancelled());
    }
    let created = session
        .call(|api, c| api.create(c, from.id, to.id, bytes))
        .map_err(err)?;
    let id = created.job.id.clone();
    tracing::info!(job = %id, "cloud job created");
    // From here on the job exists on the server: stopping must cancel it there.
    let result = session.follow(&created, job, bytes, progress, cancel, staging.path());
    match result {
        Ok(files) => {
            let pages = pages_of(&files);
            let published = destination
                .publish(
                    &files.iter().map(|(p, _)| p.clone()).collect::<Vec<_>>(),
                    &pages,
                )
                .map_err(|e| JobError::from(&e))?;
            progress(Some(1.0));
            tracing::info!(job = %id, outputs = published.len(), "cloud job done");
            Ok(published)
        }
        Err(e) => {
            if e.kind == "cancelled" || matches!(e.kind, "cloud_offline" | "io") {
                // Best effort: a job left behind expires in 24 hours anyway.
                if let Err(c) = session.call(|api, c| api.cancel(c, &id)) {
                    tracing::info!(job = %id, error = ?c, "cloud job not cancelled");
                }
            }
            tracing::info!(job = %id, kind = e.kind, "cloud job stopped");
            Err(e)
        }
    }
}

/// The page each downloaded output came from. Workers name outputs `1.png`,
/// `2.png`, ... by page, like engines do; anything else keeps its order.
fn pages_of(files: &[(PathBuf, String)]) -> Vec<usize> {
    files
        .iter()
        .enumerate()
        .map(|(n, (_, name))| {
            Path::new(name)
                .file_stem()
                .and_then(|s| s.to_str()?.parse::<usize>().ok())
                .map_or(n, |k| k.saturating_sub(1))
        })
        .collect()
}

/// One job's calls, with a credential that is renewed when it runs out.
struct Session<'a> {
    cloud: &'a Cloud,
    credential: CloudCredential,
}

impl Session<'_> {
    /// Calls the API, asking convt.app for a new credential once if the
    /// cloud refuses this one (it lasts five minutes).
    fn call<T>(
        &mut self,
        f: impl Fn(&dyn CloudApi, &CloudCredential) -> Result<T, CloudError>,
    ) -> Result<T, CloudError> {
        match f(&*self.cloud.api, &self.credential) {
            Err(e) if unauthorized(&e) => {
                self.credential = self.cloud.credentials.get(true).map_err(|_| e)?;
                f(&*self.cloud.api, &self.credential)
            }
            r => r,
        }
    }

    /// Uploads, starts, waits for and downloads a created job. Returns the
    /// downloaded files with the names the cloud gave them.
    fn follow(
        &mut self,
        created: &Created,
        job: &Job,
        bytes: u64,
        progress: &dyn Fn(Option<f32>),
        cancel: &Cancel,
        staging: &Path,
    ) -> Result<Vec<(PathBuf, String)>, JobError> {
        let to = job.to;
        let err = |e| api_error(e, to);
        let id = created.job.id.as_str();
        let api = self.cloud.api.clone();
        progress(Some(0.0));
        api.upload(
            &created.upload_url,
            &job.input,
            bytes,
            &|sent| progress(Some(0.5 * sent as f32 / bytes as f32)),
            cancel,
        )
        .map_err(err)?;
        if cancel.is_cancelled() {
            return Err(cancelled());
        }
        let mut remote = self.call(|api, c| api.start(c, id)).map_err(err)?;
        progress(None);
        let mut failing_since: Option<Instant> = None;
        loop {
            match remote.status {
                RemoteStatus::Succeeded => break,
                RemoteStatus::Failed => return Err(remote_failure(remote.error_code.as_deref())),
                RemoteStatus::Cancelled => {
                    return Err(if cancel.is_cancelled() {
                        cancelled()
                    } else {
                        fail("cloud_cancelled", "The cloud job was cancelled.")
                    });
                }
                _ => {}
            }
            if sleep(self.cloud.poll, cancel) {
                return Err(cancelled());
            }
            match self.call(|api, c| api.status(c, id)) {
                Ok(next) => {
                    failing_since = None;
                    remote = next;
                }
                Err(e) if transient(&e) => {
                    let since = *failing_since.get_or_insert_with(Instant::now);
                    if since.elapsed() > OFFLINE_GRACE {
                        return Err(err(e));
                    }
                    if sleep(self.cloud.poll * 2, cancel) {
                        return Err(cancelled());
                    }
                }
                Err(e) => return Err(err(e)),
            }
        }
        progress(Some(0.9));
        let outputs = self.call(|api, c| api.outputs(c, id)).map_err(err)?;
        if outputs.is_empty() {
            return Err(fail("cloud_failed", "convt's cloud produced no file."));
        }
        let mut files = Vec::new();
        for (i, output) in outputs.iter().enumerate() {
            if cancel.is_cancelled() {
                return Err(cancelled());
            }
            // Staged under an index, never the server's name.
            let path = staging.join(format!("{i}.part"));
            api.download(&output.url, &path, cancel).map_err(err)?;
            files.push((path, output.name.clone()));
        }
        Ok(files)
    }
}
