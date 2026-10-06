//! Explicit, verified add-on installation. Discovery and status never use the network.
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use sha2::{Digest, Sha256};

const MAX_ARCHIVE: u64 = 2 * 1024 * 1024 * 1024;
const MAX_EXTRACTED: u64 = 4 * 1024 * 1024 * 1024;

/// Release CI supplies these compile-time values for each platform. One placeholder,
/// deliberately unusable until both a real URL and a pinned digest are configured.
pub fn documents_source() -> Source {
    let mut source = Source {
        url: option_env!("CONVT_DOCUMENT_PACK_URL")
            .unwrap_or("https://documents.invalid/PLACEHOLDER/documents.tar.gz")
            .into(),
        sha256: option_env!("CONVT_DOCUMENT_PACK_SHA256")
            .unwrap_or("")
            .into(),
        version: option_env!("CONVT_DOCUMENT_PACK_VERSION")
            .unwrap_or("unconfigured")
            .into(),
    };
    if source.url == "bundle:documents.tar.gz" {
        source.url = std::env::current_exe()
            .ok()
            .and_then(|exe| {
                exe.parent()
                    .map(|dir| format!("file://{}", dir.join("documents.tar.gz").display()))
            })
            .unwrap_or_else(|| "file://missing-bundled-document-pack".into());
    }
    source
}

/// Whether this build carries a usable pin and a configured download URL.
/// The app shows a Download button only when this holds.
pub fn documents_configured() -> bool {
    valid_hash(&documents_source().sha256) && option_env!("CONVT_DOCUMENT_PACK_URL").is_some()
}

/// The pinned archive's download size and installed size in bytes, when
/// release CI supplied them (`CONVT_DOCUMENT_PACK_SIZE`,
/// `CONVT_DOCUMENT_PACK_INSTALLED_SIZE`).
pub fn documents_sizes() -> Sizes {
    let bytes = |value: Option<&str>| value.and_then(|v| v.trim().parse().ok());
    Sizes {
        download: bytes(option_env!("CONVT_DOCUMENT_PACK_SIZE")),
        installed: bytes(option_env!("CONVT_DOCUMENT_PACK_INSTALLED_SIZE")),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sizes {
    pub download: Option<u64>,
    pub installed: Option<u64>,
}

/// The error an install returns when its `cancelled` check fired during the
/// download. The partial archive is kept, so the next install resumes.
#[derive(Debug)]
pub struct Cancelled;

impl std::fmt::Display for Cancelled {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Document pack download cancelled. The next install resumes it.")
    }
}

impl std::error::Error for Cancelled {}

/// What went wrong with an install or a removal, so each client can say it
/// in its own words. [`failure_kind`] reads it from an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// The source couldn't be reached, timed out or stopped sending.
    Network,
    /// The server answered with this HTTP status.
    HttpStatus(u16),
    /// The pack folder, or a file in it, can't be written.
    Permission,
    /// The disk or the user's quota is full.
    DiskFull,
    /// The archive didn't match its pinned SHA-256 and was deleted.
    Checksum,
    /// A folder on the way to the pack, or in it, failed the ownership,
    /// permission or symlink checks.
    Rejected,
    /// Another install or removal holds the lock.
    Busy,
    Cancelled,
    Other,
}

/// An error, or the context of one, that says its [`FailureKind`].
#[derive(Debug)]
struct Tagged {
    kind: FailureKind,
    message: String,
}

impl std::fmt::Display for Tagged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Tagged {}

fn tagged(kind: FailureKind, message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(Tagged {
        kind,
        message: message.into(),
    })
}

trait Tag<T> {
    /// Marks an error with what kind of failure it is.
    fn tag(self, kind: FailureKind, message: &str) -> anyhow::Result<T>;
}

impl<T, E: Into<anyhow::Error>> Tag<T> for Result<T, E> {
    fn tag(self, kind: FailureKind, message: &str) -> anyhow::Result<T> {
        self.map_err(|e| {
            e.into().context(Tagged {
                kind,
                message: message.into(),
            })
        })
    }
}

/// The kind of failure an install or removal error is. Errors the engines
/// didn't mark are read from the I/O error underneath: no permission, or no
/// space left.
pub fn failure_kind(error: &anyhow::Error) -> FailureKind {
    if error.downcast_ref::<Cancelled>().is_some() {
        return FailureKind::Cancelled;
    }
    if let Some(tagged) = error.downcast_ref::<Tagged>() {
        return tagged.kind;
    }
    for cause in error.chain() {
        if let Some(error) = cause.downcast_ref::<ureq::Error>() {
            return match error {
                ureq::Error::StatusCode(status) => FailureKind::HttpStatus(*status),
                _ => FailureKind::Network,
            };
        }
        if let Some(kind) = cause.downcast_ref::<std::io::Error>().and_then(io_kind) {
            return kind;
        }
    }
    FailureKind::Other
}

fn io_kind(error: &std::io::Error) -> Option<FailureKind> {
    use std::io::ErrorKind as K;
    match error.kind() {
        K::PermissionDenied | K::ReadOnlyFilesystem => return Some(FailureKind::Permission),
        K::StorageFull | K::QuotaExceeded => return Some(FailureKind::DiskFull),
        _ => {}
    }
    #[cfg(unix)]
    match error.raw_os_error() {
        Some(libc::ENOSPC | libc::EDQUOT) => return Some(FailureKind::DiskFull),
        Some(libc::EACCES | libc::EPERM | libc::EROFS) => return Some(FailureKind::Permission),
        _ => {}
    }
    None
}

/// Makes convt's own data folder private before an install. Earlier app
/// versions created it with the umask (0775 on many Linux systems), and the
/// installer rightly refuses a pack root under a group-writable folder. The
/// folders above it must already pass the installer's ancestor check; then
/// the folder itself is opened without following links and changed through
/// that descriptor, so nothing can swap the path in between. Nothing else
/// is changed, and nothing is changed when a check fails.
pub fn secure_data_dir() -> anyhow::Result<()> {
    let dir = crate::paths::data_dir().context("no per-user data directory")?;
    secure_dir(&dir)
}

#[cfg(unix)]
fn secure_dir(dir: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
    let unsafe_folder = "a folder on the way to the document pack isn't safe to install into";
    let absolute = if dir.is_absolute() {
        dir.to_owned()
    } else {
        std::env::current_dir()?.join(dir)
    };
    if let Some(parent) = absolute.parent() {
        check_path(parent, false).tag(FailureKind::Rejected, unsafe_folder)?;
    }
    let folder = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_DIRECTORY)
        .open(&absolute)
    {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) if matches!(e.raw_os_error(), Some(libc::ELOOP | libc::ENOTDIR)) => {
            return Err(e)
                .context(format!(
                    "document-pack symlink forbidden: {}",
                    absolute.display()
                ))
                .tag(FailureKind::Rejected, unsafe_folder);
        }
        result => result?,
    };
    let metadata = folder.metadata()?;
    // SAFETY: geteuid has no arguments or memory access requirements.
    if metadata.uid() != unsafe { libc::geteuid() } {
        return Err(tagged(
            FailureKind::Rejected,
            format!(
                "{unsafe_folder}: document-pack path must be owned by the current user: {}",
                absolute.display()
            ),
        ));
    }
    if metadata.mode() & 0o022 != 0 {
        // fchmod on the descriptor opened above.
        folder.set_permissions(fs::Permissions::from_mode(metadata.mode() & 0o7755))?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn secure_dir(_: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[derive(Clone, Debug)]
pub struct Source {
    pub url: String,
    pub sha256: String,
    pub version: String,
}

pub enum Progress {
    Download { bytes: u64, total: Option<u64> },
    Verifying,
    Extracting,
    Installed(PathBuf),
}

fn valid_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

fn executable(dir: &Path) -> PathBuf {
    dir.join(if cfg!(windows) {
        "soffice.exe"
    } else {
        "soffice"
    })
}

pub fn documents_dir() -> Option<PathBuf> {
    crate::paths::data_dir().map(|p| p.join("packs/documents"))
}

/// Discovery uses only the digest compiled into this build.
pub fn installed_documents() -> Option<PathBuf> {
    documents_status().ok()
}

/// Offline status, including the reason an installed tree cannot be trusted.
pub fn documents_status() -> anyhow::Result<PathBuf> {
    let root = documents_dir().context("no per-user data directory")?;
    installed_in(&root)
}

fn installed_in(root: &Path) -> anyhow::Result<PathBuf> {
    installed_with_pin(root, &documents_source().sha256)
}

fn installed_with_pin(root: &Path, pin: &str) -> anyhow::Result<PathBuf> {
    if !valid_hash(pin) {
        bail!("this build has no pinned document-pack SHA-256");
    }
    trusted_root(root)?;
    let pointer = root.join("current");
    trusted_file(&pointer)?;
    let hash = fs::read_to_string(pointer)?;
    if hash.trim() != pin {
        bail!("current document-pack digest does not match this build's pin");
    }
    let dir = root.join(pin);
    trusted_tree(&dir, false)?;
    let receipt = dir.join("verified.sha256");
    trusted_file(&receipt)?;
    if fs::read_to_string(receipt)?.trim() != pin {
        bail!("document-pack receipt does not match this build's pin");
    }
    let exe = executable(&dir);
    trusted_file(&exe)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::symlink_metadata(&exe)?.permissions().mode() & 0o111 == 0 {
            bail!("document-pack launcher is not executable");
        }
    }
    Ok(exe)
}

fn trusted_metadata(path: &Path) -> anyhow::Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("document-pack path unavailable: {}", path.display()))?;
    if metadata.file_type().is_symlink() {
        bail!("document-pack symlink forbidden: {}", path.display());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // SAFETY: geteuid has no arguments or memory access requirements.
        if metadata.uid() != unsafe { libc::geteuid() } || metadata.mode() & 0o022 != 0 {
            bail!(
                "document-pack path must be owned by the current user and not group/world writable: {}",
                path.display()
            );
        }
    }
    #[cfg(windows)]
    crate::windows_acl::trusted(path, false)?;
    #[cfg(not(any(unix, windows)))]
    bail!("document-pack ownership verification is not supported on this platform");
    #[cfg(any(unix, windows))]
    Ok(metadata)
}

fn trusted_directory(path: &Path) -> anyhow::Result<()> {
    if !trusted_metadata(path)?.is_dir() {
        bail!("document-pack directory required: {}", path.display());
    }
    Ok(())
}

fn trusted_tree(root: &Path, normalize_directories: bool) -> anyhow::Result<()> {
    #[cfg(not(unix))]
    let _ = normalize_directories;
    let mut directories = vec![root.to_owned()];
    while let Some(dir) = directories.pop() {
        #[cfg(unix)]
        if normalize_directories {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::symlink_metadata(&dir)?;
            if metadata.is_dir() {
                fs::set_permissions(
                    &dir,
                    fs::Permissions::from_mode(metadata.permissions().mode() & !0o022),
                )?;
            }
        }
        trusted_directory(&dir)?;
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.is_dir() {
                directories.push(path);
            } else {
                trusted_file(&path)?;
            }
        }
    }
    Ok(())
}

fn private_tempdir(root: &Path, prefix: &str) -> anyhow::Result<tempfile::TempDir> {
    let mut builder = tempfile::Builder::new();
    builder.prefix(prefix);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o700));
    }
    Ok(builder.tempdir_in(root)?)
}

fn trusted_file(path: &Path) -> anyhow::Result<()> {
    if !trusted_metadata(path)?.is_file() {
        bail!("document-pack regular file required: {}", path.display());
    }
    Ok(())
}

fn reject_root_symlinks(root: &Path) -> anyhow::Result<()> {
    check_path(root, true)
}

/// Every ancestor must prevent other users from replacing the next
/// component. With `root`, the last component must also be a trusted pack
/// directory; without, it is checked as an ancestor.
// `last_is_root` only matters for the Unix ownership checks.
#[cfg_attr(not(unix), allow(unused_variables))]
fn check_path(root: &Path, last_is_root: bool) -> anyhow::Result<()> {
    let absolute = if root.is_absolute() {
        root.to_owned()
    } else {
        std::env::current_dir()?.join(root)
    };
    let mut path = PathBuf::new();
    for component in absolute.components() {
        if matches!(component, Component::ParentDir) {
            bail!("document-pack root must not contain parent traversal");
        }
        path.push(component);
        #[cfg(windows)]
        if matches!(component, Component::Prefix(_)) {
            // C: alone is drive-relative; inspect it only once RootDir is appended.
            continue;
        }
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                bail!("document-pack symlink forbidden: {}", path.display());
            }
            Ok(metadata) => {
                if !metadata.is_dir() {
                    bail!("document-pack directory required: {}", path.display());
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    // SAFETY: geteuid has no arguments or memory access requirements.
                    let uid = unsafe { libc::geteuid() };
                    if path == absolute && last_is_root {
                        trusted_directory(&path)?;
                    } else {
                        // Sticky ancestors such as /tmp protect the next user-owned
                        // component from rename/unlink by other directory writers.
                        let safe_owner = metadata.uid() == uid || metadata.uid() == 0;
                        let safe_mode =
                            metadata.mode() & 0o022 == 0 || metadata.mode() & 0o1000 != 0;
                        if !safe_owner || !safe_mode {
                            bail!(
                                "document-pack ancestor must be owned by the current user or root and not group/world writable unless sticky: {}",
                                path.display()
                            );
                        }
                    }
                }
                #[cfg(windows)]
                crate::windows_acl::trusted(&path, !(path == absolute && last_is_root))?;
                #[cfg(not(any(unix, windows)))]
                bail!("document-pack ownership verification is not supported on this platform");
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                #[cfg(windows)]
                if let Some(parent) = path.parent().filter(|parent| parent.exists()) {
                    // Creating siblings is safe for an existing child only. A
                    // missing next component requires a private parent first.
                    crate::windows_acl::trusted(parent, false)?;
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn trusted_root(root: &Path) -> anyhow::Result<()> {
    reject_root_symlinks(root)?;
    trusted_directory(root)
}

// A forked child can briefly retain a duplicated lock descriptor until exec.
// Release the advisory lock explicitly when the operation ends.
struct PackLock(File);

impl PackLock {
    fn acquire(path: &Path) -> anyhow::Result<Self> {
        check_existing_file(path)?;
        let file = private_options().open(path)?;
        file.try_lock().map_err(|e| {
            anyhow::Error::new(e).context(Tagged {
                kind: FailureKind::Busy,
                message: "another document-pack operation is running".into(),
            })
        })?;
        Ok(Self(file))
    }
}

impl Drop for PackLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

/// The app calls this only from the user's Install action; the CLI only from
/// `pack install`. This function never launches downloaded executables.
pub fn install_documents(source: &Source, progress: &dyn Fn(Progress)) -> anyhow::Result<PathBuf> {
    install_documents_cancellable(source, progress, &|| false)
}

/// [`install_documents`] that stops, with [`Cancelled`], once `cancelled`
/// returns true: during the download (a stalled connection included, within
/// about a quarter of a second), while a cached or finished archive is
/// verified, and before extraction starts. Extraction itself runs to the end.
pub fn install_documents_cancellable(
    source: &Source,
    progress: &dyn Fn(Progress),
    cancelled: &dyn Fn() -> bool,
) -> anyhow::Result<PathBuf> {
    let pin = documents_source().sha256;
    if !valid_hash(&pin) || source.sha256 != pin {
        bail!("Document pack must match this build's pinned SHA-256. No download was started.");
    }
    if option_env!("CONVT_DOCUMENT_PACK_URL").is_none() && source.url == documents_source().url {
        bail!("Document pack hosting is not configured in this build. No download was started.");
    }
    secure_data_dir()?;
    let root = documents_dir().context("no per-user data directory")?;
    install_at_with(&root, source, progress, cancelled)
}

#[cfg(test)]
fn install_at(
    root: &Path,
    source: &Source,
    progress: &dyn Fn(Progress),
) -> anyhow::Result<PathBuf> {
    install_at_with(root, source, progress, &|| false)
}

fn install_at_with(
    root: &Path,
    source: &Source,
    progress: &dyn Fn(Progress),
    cancelled: &dyn Fn() -> bool,
) -> anyhow::Result<PathBuf> {
    if !valid_hash(&source.sha256) {
        bail!("Document pack is not configured with a pinned SHA-256. No download was started.");
    }
    let mut directories = fs::DirBuilder::new();
    directories.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        directories.mode(0o700);
    }
    let unsafe_folder = "the document pack folder isn't safe to install into";
    reject_root_symlinks(root).tag(FailureKind::Rejected, unsafe_folder)?;
    directories.create(root)?;
    trusted_root(root).tag(FailureKind::Rejected, unsafe_folder)?;
    let _lock = PackLock::acquire(&root.join("install.lock"))?;
    let final_dir = root.join(&source.sha256);
    let partial = root.join(format!("{}.partial", source.sha256));
    check_existing_file(&partial)?;
    // A complete cached archive installs offline; verifying it can be
    // cancelled like the download.
    let complete = match File::open(&partial) {
        Ok(file) => sha256_of(file, cancelled)?.is_some_and(|hash| hash == source.sha256),
        Err(_) => false,
    };
    if !complete {
        download_with(source, &partial, progress, cancelled)?;
    }
    progress(Progress::Verifying);
    let mut archive = File::open(&partial)?;
    if sha256_of(&archive, cancelled)?.as_deref() != Some(source.sha256.as_str()) {
        fs::remove_file(&partial)?;
        return Err(tagged(
            FailureKind::Checksum,
            "Document pack SHA-256 mismatch. Removed the download; no code was installed or run.",
        ));
    }
    if cancelled() {
        return Err(Cancelled.into());
    }
    progress(Progress::Extracting);
    archive.seek(SeekFrom::Start(0))?;
    let staging = private_tempdir(root, ".install-")?;
    extract(archive, staging.path())?;
    if !executable(staging.path()).is_file() {
        bail!("Pack does not contain a soffice launcher");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if fs::metadata(executable(staging.path()))?
            .permissions()
            .mode()
            & 0o111
            == 0
        {
            bail!("Pack launcher is not executable");
        }
    }
    write_metadata(staging.path(), "verified.sha256", &source.sha256)?;
    write_metadata(staging.path(), "version", &source.version)?;
    trusted_tree(staging.path(), true)?;
    trusted_file(&executable(staging.path()))?;
    // Rebuild even when a receipt already exists. Move the old tree aside only
    // after the new archive has passed verification and extraction.
    let previous = private_tempdir(root, ".previous-")?;
    let old = previous.path().join("pack");
    if fs::symlink_metadata(&final_dir).is_ok() {
        fs::rename(&final_dir, &old)?;
    }
    if let Err(error) = fs::rename(staging.path(), &final_dir) {
        if fs::symlink_metadata(&old).is_ok() {
            fs::rename(&old, &final_dir)?;
        }
        return Err(error.into());
    }
    publish_pointer(root, &source.sha256)?;
    fs::remove_file(partial)?;
    let exe = executable(&final_dir);
    progress(Progress::Installed(exe.clone()));
    Ok(exe)
}

fn write_metadata(dir: &Path, name: &str, contents: &str) -> anyhow::Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(dir)?;
    file.write_all(contents.as_bytes())?;
    file.persist(dir.join(name))?;
    Ok(())
}

fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.create(true).truncate(false).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

fn check_existing_file(path: &Path) -> anyhow::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(_) => trusted_file(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn publish_pointer(root: &Path, hash: &str) -> anyhow::Result<()> {
    let mut current = tempfile::NamedTempFile::new_in(root)?;
    writeln!(current, "{hash}")?;
    current.as_file().sync_all()?;
    current.persist(root.join("current"))?;
    Ok(())
}

#[cfg(test)]
fn download(source: &Source, partial: &Path, progress: &dyn Fn(Progress)) -> anyhow::Result<()> {
    download_with(source, partial, progress, &|| false)
}

fn download_with(
    source: &Source,
    partial: &Path,
    progress: &dyn Fn(Progress),
    cancelled: &dyn Fn() -> bool,
) -> anyhow::Result<()> {
    if cancelled() {
        return Err(Cancelled.into());
    }
    let mut file = private_options()
        .open(partial)
        .context("couldn't save the document pack download")?;
    let mut offset = file.metadata()?.len();
    if offset > MAX_ARCHIVE {
        bail!("Pack exceeds download limit");
    }
    let result = if let Some(path) = source.url.strip_prefix("file://") {
        let unreadable = "couldn't read the document pack source";
        let mut input = File::open(path).tag(FailureKind::Network, unreadable)?;
        let len = input
            .metadata()
            .tag(FailureKind::Network, unreadable)?
            .len();
        if offset > len {
            offset = 0;
            file.set_len(0)?;
        }
        input
            .seek(SeekFrom::Start(offset))
            .tag(FailureKind::Network, unreadable)?;
        file.seek(SeekFrom::Start(offset))?;
        pump(
            &mut input,
            &mut file,
            offset,
            Some(len),
            progress,
            cancelled,
        )
    } else {
        let stop = Arc::new(AtomicBool::new(false));
        let mut body = HttpBody::start(source, offset, stop.clone(), cancelled)?;
        if body.restart {
            offset = 0;
            file.set_len(0)?;
        }
        file.seek(SeekFrom::Start(offset))?;
        let total = body.total;
        let result = pump(&mut body, &mut file, offset, total, progress, cancelled);
        // Ends the transfer thread and closes its connection, if it is still open.
        stop.store(true, Ordering::Relaxed);
        result
    };
    // Keep what arrived, even when cancelled: the next install resumes.
    let synced = file
        .sync_all()
        .context("couldn't save the document pack download");
    result?;
    synced
}

/// Copies the download into `out` block by block, reporting progress and
/// checking `cancelled` before each block and after the last. Errors from
/// `reader` are the source's (network); errors from `out` are local.
fn pump(
    reader: &mut dyn Read,
    out: &mut dyn Write,
    mut offset: u64,
    total: Option<u64>,
    progress: &dyn Fn(Progress),
    cancelled: &dyn Fn() -> bool,
) -> anyhow::Result<()> {
    if total.is_some_and(|n| n > MAX_ARCHIVE) {
        bail!("Pack exceeds download limit");
    }
    let mut block = vec![0u8; 256 * 1024];
    loop {
        if cancelled() {
            return Err(Cancelled.into());
        }
        let n = match reader.read(&mut block) {
            Ok(n) => n,
            Err(_) if cancelled() => return Err(Cancelled.into()),
            Err(e) => {
                return Err(e).tag(FailureKind::Network, "the document pack download stopped");
            }
        };
        if n == 0 {
            break;
        }
        offset += n as u64;
        if offset > MAX_ARCHIVE {
            bail!("Pack exceeds download limit");
        }
        out.write_all(&block[..n])
            .context("couldn't save the document pack download")?;
        progress(Progress::Download {
            bytes: offset,
            total,
        });
    }
    if cancelled() {
        return Err(Cancelled.into());
    }
    Ok(())
}

/// The SHA-256 of `file`, or `None` if it can't be read. Checks `cancelled`
/// between blocks.
fn sha256_of(mut file: impl Read, cancelled: &dyn Fn() -> bool) -> anyhow::Result<Option<String>> {
    let mut hash = Sha256::new();
    let mut block = vec![0u8; 1024 * 1024];
    loop {
        if cancelled() {
            return Err(Cancelled.into());
        }
        match file.read(&mut block) {
            Ok(0) => return Ok(Some(format!("{:x}", hash.finalize()))),
            Ok(n) => hash.update(&block[..n]),
            Err(_) => return Ok(None),
        }
    }
}

/// How long a read may wait before the stop flag is checked again.
const TICK: Duration = Duration::from_millis(250);
/// A connection that sends nothing for this long counts as stalled.
const STALL: Duration = Duration::from_secs(60);

enum Transfer {
    Chunk(Vec<u8>),
    End,
    Failed(anyhow::Error),
}

/// An HTTP(S) download running on its own thread, read like a file. Reads
/// wait in short slices so `cancelled` is noticed even when the server
/// stalls; setting `stop` makes the transfer thread give up within a
/// [`TICK`] and close its connection.
struct HttpBody<'a> {
    rx: mpsc::Receiver<Transfer>,
    chunk: Vec<u8>,
    at: usize,
    done: bool,
    /// The server ignored the range: the download starts over.
    restart: bool,
    total: Option<u64>,
    stop: Arc<AtomicBool>,
    cancelled: &'a dyn Fn() -> bool,
}

impl<'a> HttpBody<'a> {
    fn start(
        source: &Source,
        offset: u64,
        stop: Arc<AtomicBool>,
        cancelled: &'a dyn Fn() -> bool,
    ) -> anyhow::Result<Self> {
        // HTTPS for released packs; HTTP only for explicit loopback test sources.
        let uri: ureq::http::Uri = source.url.parse().context("Invalid pack source URL")?;
        let local_http = cfg!(test)
            && uri.scheme_str() == Some("http")
            && uri
                .host()
                .is_some_and(|h| matches!(h, "127.0.0.1" | "localhost"));
        if uri.scheme_str() != Some("https") && !local_http {
            bail!("Pack sources must use HTTPS (or file:// or loopback HTTP for local testing)");
        }
        let (response_tx, response_rx) = mpsc::channel();
        let (tx, rx) = mpsc::sync_channel(8);
        let url = source.url.clone();
        let thread_stop = stop.clone();
        std::thread::Builder::new()
            .name("convt-pack-download".into())
            .spawn(move || transfer(&url, offset, local_http, thread_stop, response_tx, tx))?;
        let mut body = HttpBody {
            rx,
            chunk: Vec::new(),
            at: 0,
            done: false,
            restart: false,
            total: None,
            stop,
            cancelled,
        };
        loop {
            if cancelled() {
                body.stop.store(true, Ordering::Relaxed);
                return Err(Cancelled.into());
            }
            match response_rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Ok((restart, total))) => {
                    body.restart = restart;
                    body.total = total;
                    return Ok(body);
                }
                Ok(Err(error)) => return Err(error),
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(tagged(
                        FailureKind::Network,
                        "the document pack download stopped",
                    ));
                }
            }
        }
    }
}

impl Read for HttpBody<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        loop {
            if self.at < self.chunk.len() {
                let n = buf.len().min(self.chunk.len() - self.at);
                buf[..n].copy_from_slice(&self.chunk[self.at..self.at + n]);
                self.at += n;
                return Ok(n);
            }
            if self.done {
                return Ok(0);
            }
            if (self.cancelled)() {
                self.stop.store(true, Ordering::Relaxed);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Interrupted,
                    Cancelled,
                ));
            }
            match self.rx.recv_timeout(Duration::from_millis(100)) {
                Ok(Transfer::Chunk(chunk)) => (self.chunk, self.at) = (chunk, 0),
                Ok(Transfer::End) => self.done = true,
                Ok(Transfer::Failed(error)) => {
                    return Err(std::io::Error::other(error.into_boxed_dyn_error()));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(std::io::ErrorKind::UnexpectedEof.into());
                }
            }
        }
    }
}

impl Drop for HttpBody<'_> {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

type ResponseResult = anyhow::Result<(bool, Option<u64>)>;

/// The transfer thread: makes the request, reports whether the download
/// restarts and its total, then sends the body in blocks until it ends, the
/// reader goes away or `stop` is set.
fn transfer(
    url: &str,
    offset: u64,
    local_http: bool,
    stop: Arc<AtomicBool>,
    response_tx: mpsc::Sender<ResponseResult>,
    tx: mpsc::SyncSender<Transfer>,
) {
    let agent = http_agent(local_http, stop.clone());
    let response = (|| -> anyhow::Result<_> {
        let mut start = offset;
        let response = match agent
            .get(url)
            .header("Range", &format!("bytes={offset}-"))
            .call()
        {
            Err(ureq::Error::StatusCode(416)) => {
                // An unverified complete or oversized partial cannot be resumed.
                // Restart once; the full archive still has to pass its pinned hash.
                start = 0;
                agent.get(url).call()?
            }
            result => result?,
        };
        let status = response.status().as_u16();
        let restart = match status {
            206 => {
                let expected = format!("bytes {start}-");
                if !response
                    .headers()
                    .get("Content-Range")
                    .and_then(|h| h.to_str().ok())
                    .is_some_and(|h| h.starts_with(&expected))
                {
                    return Err(tagged(
                        FailureKind::Network,
                        "Invalid resume range from pack server",
                    ));
                }
                start == 0 && offset != 0
            }
            200 => true,
            _ => {
                return Err(tagged(
                    FailureKind::HttpStatus(status),
                    format!("Unexpected pack download status {status}"),
                ));
            }
        };
        let total = response
            .headers()
            .get("Content-Length")
            .and_then(|h| h.to_str().ok())
            .and_then(|n| n.parse::<u64>().ok())
            .and_then(|n| n.checked_add(if restart { 0 } else { offset }));
        Ok((restart, total, response))
    })();
    let mut body = match response {
        Ok((restart, total, response)) => {
            if response_tx.send(Ok((restart, total))).is_err() {
                return;
            }
            response.into_body().into_reader()
        }
        Err(error) => {
            let _ = response_tx.send(Err(error));
            return;
        }
    };
    loop {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let mut chunk = vec![0u8; 256 * 1024];
        let message = match body.read(&mut chunk) {
            Ok(0) => Transfer::End,
            Ok(n) => {
                chunk.truncate(n);
                Transfer::Chunk(chunk)
            }
            Err(error) => Transfer::Failed(error.into()),
        };
        let last = !matches!(message, Transfer::Chunk(_));
        if tx.send(message).is_err() || last {
            return;
        }
    }
}

fn http_agent(local_http: bool, stop: Arc<AtomicBool>) -> ureq::Agent {
    use ureq::unversioned::resolver::DefaultResolver;
    use ureq::unversioned::transport::{
        ConnectProxyConnector, Connector, RustlsConnector, TcpConnector,
    };
    let config = ureq::Agent::config_builder()
        .https_only(!local_http)
        // Test HTTP stays on its explicit loopback source; do not follow it
        // to arbitrary cleartext hosts.
        .max_redirects(if local_http { 0 } else { 10 })
        .timeout_resolve(Some(Duration::from_secs(15)))
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(STALL))
        .timeout_global(Some(Duration::from_secs(1800)))
        .build();
    // The stop layer sits between the socket and TLS, so it sees every
    // wait for bytes, encrypted or not.
    let connector =
        ().chain(ConnectProxyConnector::default())
            .chain(TcpConnector::default())
            .chain(StopConnector(stop))
            .chain(RustlsConnector::default());
    ureq::Agent::with_parts(config, connector, DefaultResolver::default())
}

#[derive(Debug)]
struct StopConnector(Arc<AtomicBool>);

impl<In: ureq::unversioned::transport::Transport> ureq::unversioned::transport::Connector<In>
    for StopConnector
{
    type Out = Stoppable<In>;

    fn connect(
        &self,
        _: &ureq::unversioned::transport::ConnectionDetails,
        chained: Option<In>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        Ok(chained.map(|inner| Stoppable {
            inner,
            stop: self.0.clone(),
        }))
    }
}

/// A transport that waits for input in [`TICK`]s, gives up once `stop` is
/// set, and fails a read that gets nothing for [`STALL`].
#[derive(Debug)]
struct Stoppable<T> {
    inner: T,
    stop: Arc<AtomicBool>,
}

impl<T: ureq::unversioned::transport::Transport> ureq::unversioned::transport::Transport
    for Stoppable<T>
{
    fn buffers(&mut self) -> &mut dyn ureq::unversioned::transport::Buffers {
        self.inner.buffers()
    }

    fn transmit_output(
        &mut self,
        amount: usize,
        timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<(), ureq::Error> {
        if self.stop.load(Ordering::Relaxed) {
            return Err(stopped());
        }
        self.inner.transmit_output(amount, timeout)
    }

    fn await_input(
        &mut self,
        timeout: ureq::unversioned::transport::NextTimeout,
    ) -> Result<bool, ureq::Error> {
        let started = Instant::now();
        let deadline = timeout.not_zero().map(|d| started + *d);
        loop {
            if self.stop.load(Ordering::Relaxed) {
                return Err(stopped());
            }
            let now = Instant::now();
            if now - started > STALL {
                return Err(ureq::Error::Io(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "the document pack server stopped sending data",
                )));
            }
            let slice = match deadline {
                Some(deadline) if deadline <= now => {
                    return Err(ureq::Error::Timeout(timeout.reason));
                }
                Some(deadline) => (deadline - now).min(TICK),
                None => TICK,
            };
            let next = ureq::unversioned::transport::NextTimeout {
                after: slice.into(),
                reason: timeout.reason,
            };
            match self.inner.await_input(next) {
                Err(ureq::Error::Timeout(_)) => {}
                other => return other,
            }
        }
    }

    fn is_open(&mut self) -> bool {
        !self.stop.load(Ordering::Relaxed) && self.inner.is_open()
    }
}

fn stopped() -> ureq::Error {
    ureq::Error::Io(std::io::Error::new(
        std::io::ErrorKind::Interrupted,
        "document pack download cancelled",
    ))
}

fn extract(archive: File, destination: &Path) -> anyhow::Result<()> {
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(archive));
    let mut bytes = 0u64;
    for (count, entry) in tar.entries()?.enumerate() {
        let mut entry = entry?;
        let path = entry.path()?;
        if !path
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
        {
            bail!("Unsafe pack path");
        }
        let kind = entry.header().entry_type();
        if !kind.is_file() && !kind.is_dir() {
            bail!("Pack links and special files are forbidden");
        }
        bytes = bytes
            .checked_add(entry.size())
            .context("pack size overflow")?;
        if bytes > MAX_EXTRACTED || count >= 100_000 {
            bail!("Pack exceeds extraction limits");
        }
        #[cfg(unix)]
        {
            // Never preserve setuid/setgid or world-write permissions.
            entry.set_mask(0o7022);
        }
        if !entry.unpack_in(destination)? {
            bail!("Pack path escapes installation directory");
        }
    }
    Ok(())
}

/// Removing the pack is explicit too. System LibreOffice is never touched.
pub fn remove_documents() -> anyhow::Result<()> {
    let root = documents_dir().context("no per-user data directory")?;
    remove_at(&root)
}

fn remove_at(root: &Path) -> anyhow::Result<()> {
    reject_root_symlinks(root).tag(
        FailureKind::Rejected,
        "the document pack folder isn't safe to change",
    )?;
    match fs::symlink_metadata(root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
        Ok(_) => trusted_root(root)?,
    }
    let _lock = PackLock::acquire(&root.join("install.lock"))?;
    check_existing_file(&root.join("current"))?;
    // Validate every removal candidate before changing the installed state.
    // Orphans remain after SIGKILL, which bypasses TempDir's normal cleanup.
    let mut directories = Vec::new();
    let mut files = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let path = entry.path();
        if valid_hash(&name) || name.starts_with(".install-") || name.starts_with(".previous-") {
            trusted_tree(&path, false)?;
            directories.push(path);
        } else if name.strip_suffix(".partial").is_some_and(valid_hash) {
            trusted_file(&path)?;
            files.push(path);
        }
    }
    match fs::remove_file(root.join("current")) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    for path in directories {
        fs::remove_dir_all(path)?;
    }
    for path in files {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_tempdir() -> tempfile::TempDir {
        // macOS temp paths start with /var, a system symlink to /private/var.
        // Keep production roots strict; only resolve the test fixture parent.
        let parent = std::env::temp_dir().canonicalize().unwrap();
        private_tempdir(&parent, ".convt-pack-test-").unwrap()
    }

    fn private_root(root: &Path) {
        fs::create_dir(root).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(root, fs::Permissions::from_mode(0o700)).unwrap();
        }
    }

    fn write_private(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
        fs::write(&path, contents)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }

    fn archive(root: &Path, link: bool) -> Source {
        let path = root.join(if link { "link.tar.gz" } else { "pack.tar.gz" });
        let gzip = flate2::write::GzEncoder::new(
            File::create(&path).unwrap(),
            flate2::Compression::fast(),
        );
        let mut tar = tar::Builder::new(gzip);
        let mut header = tar::Header::new_gnu();
        let payload = b"#!/bin/sh\nexit 0\n";
        let launcher = executable(Path::new(""));
        header.set_mode(0o755);
        if link {
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.set_link_name("/tmp/escape").unwrap();
            header.set_cksum();
            tar.append_data(&mut header, &launcher, &b""[..]).unwrap();
        } else {
            header.set_size(payload.len() as u64);
            header.set_cksum();
            tar.append_data(&mut header, &launcher, &payload[..])
                .unwrap();
        }
        tar.into_inner().unwrap().finish().unwrap();
        Source {
            url: format!("file://{}", path.display()),
            sha256: format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
            version: "test".into(),
        }
    }

    #[cfg(unix)]
    #[test]
    fn followup_rejects_writable_ancestors() {
        use std::os::unix::fs::PermissionsExt;
        let temp = test_tempdir();
        let data = temp.path().join("data");
        private_root(&data);
        let root = data.join("packs/documents");
        let source = archive(temp.path(), false);
        install_at(&root, &source, &|_| {}).unwrap();
        for mode in [0o775, 0o777] {
            fs::set_permissions(&data, fs::Permissions::from_mode(mode)).unwrap();
            assert!(installed_with_pin(&root, &source.sha256).is_err());
            assert!(install_at(&root, &source, &|_| {}).is_err());
            assert!(remove_at(&root).is_err());
        }
        fs::set_permissions(&data, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(installed_with_pin(&root, &source.sha256).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn followup_removal_rejects_symlinked_root_and_lock() {
        use std::os::unix::fs::symlink;
        let temp = test_tempdir();
        let outside = temp.path().join("outside");
        private_root(&outside);
        let hash = "a".repeat(64);
        let dir = outside.join(&hash);
        private_root(&dir);
        write_private(dir.join("sentinel"), "keep").unwrap();
        write_private(outside.join("current"), &hash).unwrap();
        let root = temp.path().join("installed");
        symlink(&outside, &root).unwrap();
        assert!(remove_at(&root).is_err());
        assert!(dir.join("sentinel").exists());
        assert!(!outside.join("install.lock").exists());
        fs::remove_file(&root).unwrap();
        private_root(&root);
        let target = temp.path().join("lock-target");
        write_private(&target, "keep").unwrap();
        symlink(&target, root.join("install.lock")).unwrap();
        write_private(root.join("current"), &hash).unwrap();
        assert!(remove_at(&root).is_err());
        assert!(root.join("current").exists());
        assert_eq!(fs::read_to_string(&target).unwrap(), "keep");
    }

    #[cfg(unix)]
    #[test]
    fn followup_removal_cleans_crash_orphans() {
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        private_root(&root);
        for name in [".install-interrupted", ".previous-interrupted"] {
            let orphan = root.join(name);
            private_root(&orphan);
            let nested = orphan.join("pack");
            private_root(&nested);
            write_private(nested.join("sentinel"), "old pack").unwrap();
        }
        let unrelated = root.join("keep");
        private_root(&unrelated);
        remove_at(&root).unwrap();
        assert!(!root.join(".install-interrupted").exists());
        assert!(!root.join(".previous-interrupted").exists());
        assert!(unrelated.exists());
    }

    #[cfg(unix)]
    #[test]
    fn removal_rejects_unsafe_orphans_without_following_links() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        private_root(&root);
        let outside = temp.path().join("outside");
        private_root(&outside);
        write_private(outside.join("sentinel"), "keep").unwrap();
        let orphan = root.join(".install-interrupted");
        symlink(&outside, &orphan).unwrap();
        assert!(remove_at(&root).is_err());
        assert!(
            fs::symlink_metadata(&orphan)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        fs::remove_file(&orphan).unwrap();
        private_root(&orphan);
        symlink(&outside, orphan.join("nested")).unwrap();
        assert!(remove_at(&root).is_err());
        assert_eq!(
            fs::read_to_string(outside.join("sentinel")).unwrap(),
            "keep"
        );
        fs::remove_file(orphan.join("nested")).unwrap();
        fs::set_permissions(&orphan, fs::Permissions::from_mode(0o775)).unwrap();
        assert!(remove_at(&root).is_err());
        assert!(orphan.exists());
        fs::set_permissions(&orphan, fs::Permissions::from_mode(0o700)).unwrap();
        remove_at(&root).unwrap();
        assert!(!orphan.exists());
    }

    #[cfg(unix)]
    #[test]
    fn removal_holds_lock_before_cleaning_orphans() {
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        private_root(&root);
        let orphan = root.join(".previous-interrupted");
        private_root(&orphan);
        let lock = private_options().open(root.join("install.lock")).unwrap();
        lock.try_lock().unwrap();
        assert!(
            remove_at(&root)
                .unwrap_err()
                .to_string()
                .contains("another")
        );
        assert!(orphan.exists());
        drop(lock);
        remove_at(&root).unwrap();
        assert!(!orphan.exists());
    }

    #[cfg(not(unix))]
    #[test]
    fn unsupported_ownership_rejects_pack_operations_before_download() {
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        private_root(&root);
        let source = Source {
            url: "https://unreachable.invalid/pack".into(),
            sha256: "a".repeat(64),
            version: "test".into(),
        };
        for error in [
            install_at(&root, &source, &|_| panic!("must not download")).unwrap_err(),
            remove_at(&root).unwrap_err(),
        ] {
            assert_eq!(failure_kind(&error), FailureKind::Rejected);
            assert!(format!("{error:#}").contains("isn't safe"), "{error:#}");
        }
        assert!(!root.join("install.lock").exists());
    }

    #[test]
    fn rejects_forged_pack_discovery() {
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        let hash = "a".repeat(64);
        let dir = root.join(&hash);
        fs::create_dir_all(&dir).unwrap();
        write_private(root.join("current"), &hash).unwrap();
        write_private(dir.join("verified.sha256"), &hash).unwrap();
        write_private(executable(&dir), "forged launcher").unwrap();
        assert!(
            installed_in(&root).is_err(),
            "arbitrary receipt was trusted"
        );
    }

    #[cfg(unix)]
    #[test]
    fn completed_operation_releases_lock_even_with_duplicated_descriptor() {
        let temp = test_tempdir();
        let path = temp.path().join("install.lock");
        let operation = PackLock::acquire(&path).unwrap();
        let inherited = operation.0.try_clone().unwrap();
        drop(operation);
        let next = PackLock::acquire(&path);
        drop(inherited);
        assert!(
            next.is_ok(),
            "finished operation left its lock in an inherited descriptor"
        );
    }

    #[cfg(unix)]
    #[test]
    fn reinstall_rebuilds_tampered_launcher_from_verified_archive() {
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        let source = archive(temp.path(), false);
        let exe = install_at(&root, &source, &|_| {}).unwrap();
        write_private(&exe, "tampered launcher").unwrap();
        install_at(&root, &source, &|_| {}).unwrap();
        assert_eq!(fs::read(&exe).unwrap(), b"#!/bin/sh\nexit 0\n");
    }

    #[cfg(unix)]
    #[test]
    fn discovery_requires_the_expected_pin_and_receipt() {
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        let source = archive(temp.path(), false);
        install_at(&root, &source, &|_| {}).unwrap();
        assert!(
            installed_with_pin(&root, "")
                .unwrap_err()
                .to_string()
                .contains("no pinned")
        );
        assert!(
            installed_with_pin(&root, &"0".repeat(64))
                .unwrap_err()
                .to_string()
                .contains("digest")
        );
        write_private(
            root.join(&source.sha256).join("verified.sha256"),
            "0".repeat(64),
        )
        .unwrap();
        assert!(
            installed_with_pin(&root, &source.sha256)
                .unwrap_err()
                .to_string()
                .contains("receipt")
        );
    }

    #[cfg(unix)]
    #[test]
    fn discovery_rejects_writable_paths_and_symlinks() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        let source = archive(temp.path(), false);
        let exe = install_at(&root, &source, &|_| {}).unwrap();
        let dir = root.join(&source.sha256);
        let nested = dir.join("lib/nested");
        fs::create_dir_all(&nested).unwrap();
        fs::set_permissions(dir.join("lib"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&nested, fs::Permissions::from_mode(0o700)).unwrap();
        for path in [
            &root,
            &dir,
            &nested,
            &exe,
            &root.join("current"),
            &dir.join("verified.sha256"),
        ] {
            let original = fs::metadata(path).unwrap().permissions();
            for mode in [0o777, 0o775] {
                fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
                assert!(
                    installed_with_pin(&root, &source.sha256)
                        .unwrap_err()
                        .to_string()
                        .contains("writable"),
                    "{}",
                    path.display()
                );
            }
            fs::set_permissions(path, original).unwrap();
        }
        let outside = temp.path().join("outside");
        write_private(&outside, "outside launcher").unwrap();
        for path in [
            &exe,
            &root.join("current"),
            &dir.join("verified.sha256"),
            &nested,
            &dir,
            &root,
        ] {
            let saved = path.with_extension("saved");
            fs::rename(path, &saved).unwrap();
            symlink(&outside, path).unwrap();
            assert!(
                installed_with_pin(&root, &source.sha256)
                    .unwrap_err()
                    .to_string()
                    .contains("symlink"),
                "{}",
                path.display()
            );
            fs::remove_file(path).unwrap();
            fs::rename(saved, path).unwrap();
        }
        fs::remove_file(&exe).unwrap();
        fs::create_dir(&exe).unwrap();
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(
            installed_with_pin(&root, &source.sha256)
                .unwrap_err()
                .to_string()
                .contains("regular file")
        );
    }

    #[cfg(unix)]
    #[test]
    fn reinstall_does_not_publish_a_forged_receipt_without_an_archive() {
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        let mut source = archive(temp.path(), false);
        let exe = install_at(&root, &source, &|_| {}).unwrap();
        write_private(&exe, "forged launcher").unwrap();
        fs::remove_file(root.join("current")).unwrap();
        source.url = "file:///missing-verified-archive".into();
        assert!(install_at(&root, &source, &|_| {}).is_err());
        assert!(!root.join("current").exists());
        assert_eq!(fs::read_to_string(exe).unwrap(), "forged launcher");
    }

    #[cfg(unix)]
    #[test]
    fn cancelled_download_keeps_its_partial_and_resumes() {
        use std::cell::Cell;
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        private_root(&root);
        let mut source = archive(temp.path(), false);
        // Grow the archive past one block so the download has a middle.
        let path = PathBuf::from(source.url.strip_prefix("file://").unwrap());
        let mut bytes = fs::read(&path).unwrap();
        bytes.extend(std::iter::repeat_n(0u8, 600 * 1024));
        fs::write(&path, &bytes).unwrap();
        source.sha256 = format!("{:x}", Sha256::digest(&bytes));
        let blocks = Cell::new(0);
        let error = install_at_with(
            &root,
            &source,
            &|event| {
                if matches!(event, Progress::Download { .. }) {
                    blocks.set(blocks.get() + 1);
                }
            },
            &|| blocks.get() >= 1,
        )
        .unwrap_err();
        assert!(error.downcast_ref::<Cancelled>().is_some(), "{error:#}");
        let partial = root.join(format!("{}.partial", source.sha256));
        assert_eq!(fs::metadata(&partial).unwrap().len(), 256 * 1024);
        assert!(installed_with_pin(&root, &source.sha256).is_err());
        // Cancelled before the first byte: nothing is read at all.
        let error = install_at_with(&root, &source, &|_| {}, &|| true).unwrap_err();
        assert!(error.downcast_ref::<Cancelled>().is_some(), "{error:#}");
        assert_eq!(fs::metadata(&partial).unwrap().len(), 256 * 1024);
        // The next install resumes after the kept block and verifies the
        // whole archive.
        let first = Cell::new(None);
        let exe = install_at(&root, &source, &|event| {
            if let Progress::Download { bytes, .. } = event {
                first.set(first.get().or(Some(bytes)));
            }
        })
        .unwrap();
        assert_eq!(first.get(), Some(2 * 256 * 1024));
        assert_eq!(installed_with_pin(&root, &source.sha256).unwrap(), exe);
    }

    /// Reads one HTTP request head from `stream`.
    #[cfg(unix)]
    fn read_request(stream: &mut std::net::TcpStream) -> String {
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        String::from_utf8(request).unwrap()
    }

    // Only the Unix permission tests ask.
    #[cfg(unix)]
    fn not_root() -> bool {
        // SAFETY: geteuid has no arguments or memory access requirements.
        unsafe { libc::geteuid() != 0 }
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_pack_folder_is_a_permission_failure() {
        use std::os::unix::fs::PermissionsExt;
        if !not_root() {
            return;
        }
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        private_root(&root);
        write_private(root.join("install.lock"), "").unwrap();
        fs::set_permissions(root.join("install.lock"), fs::Permissions::from_mode(0o600)).unwrap();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o500)).unwrap();
        let source = archive(temp.path(), false);
        let error = install_at(&root, &source, &|_| {}).unwrap_err();
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(failure_kind(&error), FailureKind::Permission, "{error:#}");
        assert!(!format!("{error:#}").contains("connection"), "{error:#}");
    }

    #[test]
    fn a_full_disk_is_a_disk_full_failure() {
        struct Full;
        impl Write for Full {
            fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
                #[cfg(unix)]
                return Err(std::io::Error::from_raw_os_error(libc::ENOSPC));
                #[cfg(not(unix))]
                return Err(std::io::ErrorKind::StorageFull.into());
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut input = &[7u8; 1000][..];
        let error = pump(&mut input, &mut Full, 0, Some(1000), &|_| {}, &|| false).unwrap_err();
        assert_eq!(failure_kind(&error), FailureKind::DiskFull, "{error:#}");
    }

    #[cfg(unix)]
    #[test]
    fn failures_say_what_kind_they_are() {
        use std::net::TcpListener;
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        private_root(&root);
        let mut source = archive(temp.path(), false);

        // Nothing listens on the port: a network failure.
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        source.url = format!("http://127.0.0.1:{port}/pack");
        let error = install_at(&root, &source, &|_| {}).unwrap_err();
        assert_eq!(failure_kind(&error), FailureKind::Network, "{error:#}");

        // The server answers 500.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        source.url = format!("http://{}/pack", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&mut stream);
            write!(stream, "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let error = install_at(&root, &source, &|_| {}).unwrap_err();
        server.join().unwrap();
        assert_eq!(
            failure_kind(&error),
            FailureKind::HttpStatus(500),
            "{error:#}"
        );

        // Bytes that don't match the pin.
        let mut wrong = archive(temp.path(), false);
        wrong.sha256 = "0".repeat(64);
        let error = install_at(&root, &wrong, &|_| {}).unwrap_err();
        assert_eq!(failure_kind(&error), FailureKind::Checksum, "{error:#}");

        // Another install holds the lock.
        let lock = PackLock::acquire(&root.join("install.lock")).unwrap();
        let error = install_at(&root, &archive(temp.path(), false), &|_| {}).unwrap_err();
        assert_eq!(failure_kind(&error), FailureKind::Busy, "{error:#}");
        drop(lock);
    }

    #[cfg(unix)]
    #[test]
    fn a_stalled_download_cancels_within_moments_and_lets_go() {
        use std::net::TcpListener;
        use std::sync::atomic::AtomicU64;
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        private_root(&root);
        let mut source = archive(temp.path(), false);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        source.url = format!("http://{}/pack", listener.local_addr().unwrap());
        // Sends a kilobyte of a megabyte, then stalls; reports when the
        // client hangs up.
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&mut stream);
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: 1048576\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
            stream.write_all(&[1; 1024]).unwrap();
            stream.flush().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(20)))
                .unwrap();
            let mut byte = [0];
            let closed = matches!(stream.read(&mut byte), Ok(0) | Err(_));
            (closed, Instant::now())
        });
        let cancel_at = AtomicU64::new(0);
        let start = Instant::now();
        let error = install_at_with(
            &root,
            &source,
            &|event| {
                if matches!(event, Progress::Download { .. }) {
                    // Cancel half a second after the stall begins.
                    let at = start.elapsed().as_millis() as u64 + 500;
                    let _ = cancel_at.compare_exchange(0, at, Ordering::SeqCst, Ordering::SeqCst);
                }
            },
            &|| {
                let at = cancel_at.load(Ordering::SeqCst);
                at != 0 && start.elapsed().as_millis() as u64 >= at
            },
        )
        .unwrap_err();
        let returned = Instant::now();
        let cancelled = start + Duration::from_millis(cancel_at.load(Ordering::SeqCst));
        assert_eq!(failure_kind(&error), FailureKind::Cancelled, "{error:#}");
        assert!(
            returned - cancelled < Duration::from_secs(2),
            "took {:?} after cancel",
            returned - cancelled
        );
        // The lock is free and the connection closes.
        drop(PackLock::acquire(&root.join("install.lock")).unwrap());
        let (closed, at) = server.join().unwrap();
        assert!(closed);
        assert!(
            at - cancelled < Duration::from_secs(2),
            "{:?}",
            at - cancelled
        );
        let partial = root.join(format!("{}.partial", source.sha256));
        assert_eq!(fs::metadata(partial).unwrap().len(), 1024);
    }

    #[cfg(unix)]
    #[test]
    fn verifying_a_cached_archive_can_be_cancelled() {
        use std::cell::Cell;
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        private_root(&root);
        let mut source = archive(temp.path(), false);
        source.url = "file:///never-read".into();
        let partial = root.join(format!("{}.partial", source.sha256));
        write_private(&partial, vec![0u8; 3 * 1024 * 1024]).unwrap();
        let checks = Cell::new(0);
        let error = install_at_with(&root, &source, &|_| {}, &|| {
            checks.set(checks.get() + 1);
            checks.get() > 1
        })
        .unwrap_err();
        assert_eq!(failure_kind(&error), FailureKind::Cancelled, "{error:#}");
        assert_eq!(fs::metadata(&partial).unwrap().len(), 3 * 1024 * 1024);
    }

    #[cfg(windows)]
    #[test]
    fn missing_pack_parents_require_private_existing_parent() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("missing").join("packs");
        check_path(&root, true).unwrap();
        let result = std::process::Command::new("icacls")
            .arg(temp.path())
            .args(["/grant", "*S-1-1-0:(AD)"])
            .output()
            .unwrap();
        assert!(result.status.success(), "{result:?}");
        let error = check_path(&root, true).unwrap_err();
        assert!(
            error.to_string().contains("writable by another principal"),
            "{error:#}"
        );
        assert!(!root.parent().unwrap().exists());
    }

    #[cfg(unix)]
    #[test]
    fn the_data_folder_is_made_private_only_behind_safe_folders() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let temp = test_tempdir();
        let mode = |p: &Path| fs::symlink_metadata(p).unwrap().permissions().mode() & 0o7777;
        let set = |p: &Path, m| fs::set_permissions(p, fs::Permissions::from_mode(m)).unwrap();

        // An umask-made folder under safe ones loses group and world write.
        let share = temp.path().join("share");
        private_root(&share);
        let data = share.join("convt");
        fs::create_dir(&data).unwrap();
        set(&data, 0o775);
        secure_dir(&data).unwrap();
        assert_eq!(mode(&data), 0o755);
        secure_dir(&share.join("missing")).unwrap();

        // A symlinked parent (a linked XDG_DATA_HOME): nothing changes.
        let linked = temp.path().join("linked-share");
        symlink(&share, &linked).unwrap();
        set(&data, 0o775);
        let error = secure_dir(&linked.join("convt")).unwrap_err();
        assert_eq!(failure_kind(&error), FailureKind::Rejected, "{error:#}");
        assert_eq!(mode(&data), 0o775);

        // The folder itself a link: not followed.
        let link = share.join("convt-link");
        symlink(&data, &link).unwrap();
        let error = secure_dir(&link).unwrap_err();
        assert_eq!(failure_kind(&error), FailureKind::Rejected, "{error:#}");
        assert_eq!(mode(&data), 0o775);

        // A group-writable parent: nothing changes.
        set(&share, 0o775);
        let error = secure_dir(&data).unwrap_err();
        assert_eq!(failure_kind(&error), FailureKind::Rejected, "{error:#}");
        assert_eq!(mode(&data), 0o775);
        set(&share, 0o700);
    }

    #[test]
    fn loopback_test_download_does_not_follow_redirects() {
        use std::net::TcpListener;
        let temp = test_tempdir();
        let mut source = archive(temp.path(), false);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        source.url = format!("http://{}/pack", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            write!(stream, "HTTP/1.1 302 Found\r\nLocation: http://untrusted.invalid/pack\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
        });
        let error = download(&source, &temp.path().join("download"), &|_| {}).unwrap_err();
        server.join().unwrap();
        assert!(
            error
                .to_string()
                .contains("Unexpected pack download status 302"),
            "{error:#}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn verifies_resumes_and_publishes_without_running_code() {
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        private_root(&root);
        let source = archive(temp.path(), false);
        let bytes = fs::read(source.url.strip_prefix("file://").unwrap()).unwrap();
        write_private(
            root.join(format!("{}.partial", source.sha256)),
            &bytes[..20],
        )
        .unwrap();
        let exe = install_at(&root, &source, &|_| {}).unwrap();
        assert_eq!(installed_with_pin(&root, &source.sha256).unwrap(), exe);
        assert!(!root.join(format!("{}.partial", source.sha256)).exists());
        assert_eq!(
            fs::read_to_string(root.join(&source.sha256).join("version")).unwrap(),
            "test"
        );
    }

    #[cfg(unix)]
    #[test]
    fn complete_download_and_interrupted_publication_recover_offline() {
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        private_root(&root);
        let mut source = archive(temp.path(), false);
        let bytes = fs::read(source.url.strip_prefix("file://").unwrap()).unwrap();
        write_private(root.join(format!("{}.partial", source.sha256)), &bytes).unwrap();
        source.url = "file:///does-not-exist".into();
        let exe = install_at(&root, &source, &|_| {}).unwrap();
        fs::remove_file(root.join("current")).unwrap();
        assert!(installed_in(&root).is_err());
        assert!(install_at(&root, &source, &|_| {}).is_err());
        write_private(root.join(format!("{}.partial", source.sha256)), &bytes).unwrap();
        assert_eq!(install_at(&root, &source, &|_| {}).unwrap(), exe);
        assert_eq!(installed_with_pin(&root, &source.sha256).unwrap(), exe);
    }

    #[cfg(unix)]
    #[test]
    fn resumes_http_from_the_requested_byte_offset() {
        use std::net::TcpListener;
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        private_root(&root);
        let mut source = archive(temp.path(), false);
        let bytes = fs::read(source.url.strip_prefix("file://").unwrap()).unwrap();
        write_private(
            root.join(format!("{}.partial", source.sha256)),
            &bytes[..20],
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        source.url = format!("http://{}/pack", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut request = Vec::new();
            loop {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
                if request.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            assert!(
                String::from_utf8(request)
                    .unwrap()
                    .to_lowercase()
                    .contains("range: bytes=20-")
            );
            write!(stream, "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\nContent-Range: bytes 20-{}/{}\r\nConnection: close\r\n\r\n", bytes.len()-20, bytes.len()-1, bytes.len()).unwrap();
            stream.write_all(&bytes[20..]).unwrap();
        });
        assert!(install_at(&root, &source, &|_| {}).is_ok());
        server.join().unwrap();
        assert!(installed_with_pin(&root, &source.sha256).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn corrupt_full_partial_restarts_after_http_416() {
        use std::net::TcpListener;
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        private_root(&root);
        let mut source = archive(temp.path(), false);
        let bytes = fs::read(source.url.strip_prefix("file://").unwrap()).unwrap();
        write_private(
            root.join(format!("{}.partial", source.sha256)),
            vec![0; bytes.len()],
        )
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        source.url = format!("http://{}/pack", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for attempt in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(10)))
                    .unwrap();
                let mut request = Vec::new();
                loop {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                    if request.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                if attempt == 0 {
                    assert!(
                        String::from_utf8(request)
                            .unwrap()
                            .contains(&format!("bytes={}-", bytes.len()))
                    );
                    write!(stream, "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                } else {
                    write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        bytes.len()
                    )
                    .unwrap();
                    stream.write_all(&bytes).unwrap();
                }
            }
        });
        assert!(install_at(&root, &source, &|_| {}).is_ok());
        server.join().unwrap();
        assert!(installed_with_pin(&root, &source.sha256).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_wrong_hash_and_links_before_publication() {
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        let mut source = archive(temp.path(), false);
        source.sha256 = "0".repeat(64);
        assert!(
            install_at(&root, &source, &|_| {})
                .unwrap_err()
                .to_string()
                .contains("SHA-256 mismatch")
        );
        assert!(installed_in(&root).is_err());
        assert!(!root.join(format!("{}.partial", source.sha256)).exists());
        let source = archive(temp.path(), true);
        assert!(
            install_at(&root, &source, &|_| {})
                .unwrap_err()
                .to_string()
                .contains("links")
        );
        assert!(installed_in(&root).is_err());
    }

    #[test]
    fn unconfigured_source_and_corrupt_pointer_are_offline() {
        let temp = test_tempdir();
        let root = temp.path().join("installed");
        let source = Source {
            url: "https://unreachable.invalid".into(),
            sha256: "".into(),
            version: "test".into(),
        };
        assert!(
            install_at(&root, &source, &|_| {})
                .unwrap_err()
                .to_string()
                .contains("No download")
        );
        assert!(!root.exists());
        private_root(&root);
        write_private(root.join("current"), "../../escape").unwrap();
        assert!(installed_in(&root).is_err());
    }
}
