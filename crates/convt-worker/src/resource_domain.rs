//! Aggregate Linux resource limits for a job and all of its descendants.
//!
//! Startup requires a writable, delegated cgroup-v2 container scope with cpu,
//! memory and pids controllers, and permission to mount tmpfs (CAP_SYS_ADMIN and
//! a compatible seccomp/AppArmor policy). The kernel must expose memory.swap.max
//! and memory.oom.group; the output ancestor must not be a shared mount. Railway
//! support for these requirements is NOT CHECKED. This
//! module has no unsafe fallback; the caller owns any explicit unsafe override.
//! Call startup before spawning jobs, with no concurrent process churn in the
//! container leaf. The caller retains its TempDir until explicit cleanup succeeds.
//! For scoped Docker testing, use cgroupns=host, bind only that container's own
//! cgroup read-write at /sys/fs/cgroup, set CONVT_SANDBOX_CGROUP_ROOT to that
//! mountpoint, and grant SYS_ADMIN with a mount-compatible AppArmor policy.
//! Never bind the host-wide cgroup tree read-write or use nested Docker.
use anyhow::{Context, Result, ensure};
use std::{
    ffi::CString,
    fs,
    os::unix::{
        ffi::OsStrExt,
        fs::{MetadataExt, PermissionsExt},
    },
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
};

const CONTROLLERS: [&str; 3] = ["cpu", "memory", "pids"];
const MEMORY: u64 = 4_000_000_000;

#[derive(Clone, Debug)]
pub struct ResourceDomain {
    root: Arc<PathBuf>,
}

/// Open cgroup_procs before chroot; after the guardian fork, the conversion
/// child writes its own PID (or "0") to that descriptor, then closes it before
/// dropping UID/capabilities. The guardian and worker must never enter this leaf.
/// All descendants inherit its aggregate limits. This object does not kill PIDs
/// and has no implicit Drop cleanup: the caller must reap the job, then cleanup.
#[derive(Debug)]
pub struct JobResources {
    pub cgroup_procs: PathBuf,
    domain: Arc<PathBuf>,
    cgroup: PathBuf,
    output: PathBuf,
    identity: (u64, u64),
    state: Mutex<CleanupState>,
}

#[derive(Debug)]
struct CleanupState {
    mount_id: Option<u64>,
    removed: bool,
}

fn scope_allowed(
    selected: &Path,
    current: &Path,
    mount_root: &Path,
    verified_migration: bool,
) -> bool {
    selected.starts_with(current)
        || (mount_root != Path::new("/")
            && current.parent() == Some(selected)
            && verified_migration)
}

// A later supervisor may select its original scope only when this root parent
// recorded that exact migration. A non-root bind alone cannot prove ownership.
fn proof_path(scope: &Path) -> Result<PathBuf> {
    let directory = Path::new("/run/convt-cgroup-scopes");
    match fs::create_dir(directory) {
        Ok(()) => fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let metadata = fs::symlink_metadata(directory)?;
    ensure!(
        metadata.is_dir() && metadata.uid() == 0 && metadata.mode() & 0o077 == 0,
        "cgroup proof directory must be private and root-owned"
    );
    let scope = fs::metadata(scope)?;
    Ok(directory.join(format!("{}-{}", scope.dev(), scope.ino())))
}
fn verified_migration(scope: &Path, current: &Path) -> Result<bool> {
    if current.parent() != Some(scope) {
        return Ok(false);
    }
    let file = proof_path(scope)?;
    let metadata = match fs::symlink_metadata(&file) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    ensure!(
        metadata.is_file() && metadata.uid() == 0 && metadata.mode() & 0o077 == 0,
        "cgroup migration proof must be private and root-owned"
    );
    let recorded: (PathBuf, u64, u64) = serde_json::from_slice(&fs::read(file)?)?;
    let actual = fs::metadata(current)?;
    Ok(recorded == (current.to_path_buf(), actual.dev(), actual.ino()))
}
fn record_migration(scope: &Path, supervisor: &Path) -> Result<()> {
    use std::io::Write;
    let file = proof_path(scope)?;
    let metadata = fs::metadata(supervisor)?;
    let mut pending = tempfile::NamedTempFile::new_in(file.parent().context("proof parent")?)?;
    pending.write_all(&serde_json::to_vec(&(
        supervisor,
        metadata.dev(),
        metadata.ino(),
    ))?)?;
    pending
        .persist_noclobber(file)
        .context("record original supervisor scope")?;
    Ok(())
}

impl ResourceDomain {
    pub fn from_env() -> Result<Self> {
        ensure!(
            unsafe { libc::geteuid() } == 0,
            "resource domains require root"
        );
        let mount = Path::new("/sys/fs/cgroup");
        let info = mount_info(mount)?;
        ensure!(info.fs_type == "cgroup2", "cgroup v2 is required");
        let current = current_cgroup()?;
        let relative = current.strip_prefix(&info.root).context(
            "current cgroup is outside the /sys/fs/cgroup mount; use a scoped container bind",
        )?;
        let current_dir = fs::canonicalize(mount.join(relative))?;
        let mount = fs::canonicalize(mount)?;
        ensure!(current_dir.starts_with(&mount), "cgroup path escaped mount");
        let scope = match std::env::var_os("CONVT_SANDBOX_CGROUP_ROOT") {
            Some(value) => {
                ensure!(
                    current != Path::new("/") || info.root != Path::new("/"),
                    "a namespace or host root is not proof of delegation; use cgroupns=host and a bind of only the current container scope"
                );
                let selected = fs::canonicalize(PathBuf::from(value))
                    .context("resolve CONVT_SANDBOX_CGROUP_ROOT")?;
                ensure!(
                    scope_allowed(
                        &selected,
                        &current_dir,
                        &info.root,
                        verified_migration(&selected, &current_dir)?
                    ),
                    "explicit cgroup root must be within the current container cgroup"
                );
                // A scoped bind mount exposes its real non-root hierarchy path.
                // Merely pointing at a host-wide mount is not delegation proof.
                ensure!(
                    selected != mount || info.root != Path::new("/"),
                    "explicit root is not a demonstrably delegated scope; bind only the container cgroup"
                );
                selected
            }
            None => {
                ensure!(
                    current != Path::new("/") && current_dir != mount,
                    "refusing cgroup root without an explicitly delegated CONVT_SANDBOX_CGROUP_ROOT"
                );
                current_dir.clone()
            }
        };
        ensure!(
            scope.join("cgroup.type").exists(),
            "missing cgroup v2 scope"
        );
        ensure!(
            fs::read_to_string(scope.join("cgroup.type"))?.trim() == "domain",
            "a domain cgroup is required"
        );
        let available = fs::read_to_string(scope.join("cgroup.controllers"))?;
        for controller in CONTROLLERS {
            ensure!(
                available.split_whitespace().any(|c| c == controller),
                "{controller} controller is not delegated"
            );
        }
        // Actual mkdir is the delegation probe, not an access-bit/CAP check.
        let root = unique_cgroup(&scope, "convt-jobs")?;
        let setup = (|| {
            let enabled = fs::read_to_string(scope.join("cgroup.subtree_control"))?;
            if CONTROLLERS
                .iter()
                .any(|c| !enabled.split_whitespace().any(|e| e == *c))
            {
                let pids = read_pids(&scope)?;
                if !pids.is_empty() {
                    ensure!(
                        scope == current_dir,
                        "cannot evacuate an explicit scope other than the current container leaf"
                    );
                    // Validate the entire snapshot before moving any process.
                    for &pid in &pids {
                        validate_owned_pid(pid, &current)?;
                    }
                    let supervisor = unique_cgroup(&scope, "convt-supervisor")?;
                    for pid in pids {
                        // Exited processes need no migration. Never operate on
                        // another cgroup or on a process owned by another UID.
                        match validate_owned_pid(pid, &current) {
                            Ok(()) => write(&supervisor, "cgroup.procs", &pid.to_string())?,
                            Err(error) if !Path::new(&format!("/proc/{pid}")).exists() => {
                                let _ = error;
                            }
                            Err(error) => return Err(error),
                        }
                    }
                    ensure!(
                        read_pids(&scope)?.is_empty(),
                        "container leaf changed during startup; stop process churn before enabling controllers"
                    );
                    record_migration(&scope, &supervisor)?;
                }
                write(&scope, "cgroup.subtree_control", "+cpu +memory +pids")?;
            }
            write(&root, "cgroup.subtree_control", "+cpu +memory +pids")?;
            // Exercise the exact job setup, including a real tmpfs mount. The
            // probe lives outside the checkout and is explicitly torn down.
            let domain = Self {
                root: Arc::new(root.clone()),
            };
            let output = tempfile::tempdir().context("create tmpfs startup probe")?;
            let probe = domain.create(10000, output.path())?;
            probe.cleanup().context("cleanup tmpfs startup probe")?;
            Ok(domain)
        })();
        if setup.is_err() {
            // Never roll back supervisor migration: those are live processes.
            // Removing an empty owned parent is safe; nonempty removal fails.
            let _ = fs::remove_dir(&root);
        }
        setup
    }

    pub fn create(&self, uid: u32, output: &Path) -> Result<JobResources> {
        ensure!(uid >= 10000, "unprivileged job uid required");
        let metadata = fs::symlink_metadata(output)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "output must be a real directory"
        );
        ensure!(
            metadata.uid() == unsafe { libc::geteuid() } && metadata.mode() & 0o022 == 0,
            "output must be supervisor-owned and not writable by other users"
        );
        ensure!(
            fs::read_dir(output)?.next().is_none(),
            "output mountpoint must be empty"
        );
        let output = fs::canonicalize(output)?;
        // A mount under a shared ancestor could propagate into the host. Do
        // not change unrelated mount propagation to make this check pass.
        ensure_private_ancestor(&output)?;
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        ensure!(page_size > 0, "page size unavailable");
        let memory = (MEMORY / page_size as u64 * page_size as u64).to_string();
        let cgroup = unique_cgroup(&self.root, "job")?;
        let setup = (|| {
            write(&cgroup, "cpu.max", "200000 100000")?;
            write(&cgroup, "memory.max", &memory)?;
            write(&cgroup, "memory.swap.max", "0")?;
            write(&cgroup, "memory.oom.group", "1")?;
            write(&cgroup, "pids.max", "128")?;
            for (name, expected) in [
                ("cpu.max", "200000 100000"),
                ("memory.max", memory.as_str()),
                ("memory.swap.max", "0"),
                ("memory.oom.group", "1"),
                ("pids.max", "128"),
            ] {
                ensure!(
                    fs::read_to_string(cgroup.join(name))?.trim() == expected,
                    "kernel did not apply {name}"
                );
            }
            let metadata = fs::metadata(&cgroup)?;
            mount_tmpfs(&output, uid)?;
            let mounted = mount_info(&output);
            match mounted {
                Ok(info) => Ok(JobResources {
                    cgroup_procs: cgroup.join("cgroup.procs"),
                    domain: self.root.clone(),
                    cgroup: cgroup.clone(),
                    output: output.clone(),
                    identity: (metadata.dev(), metadata.ino()),
                    state: Mutex::new(CleanupState {
                        mount_id: Some(info.id),
                        removed: false,
                    }),
                }),
                Err(error) => {
                    unmount(&output).context("unmount job after mount inspection failure")?;
                    Err(error)
                }
            }
        })();
        if setup.is_err() {
            // rmdir does not remove descendants or terminate any process.
            fs::remove_dir(&cgroup).context("remove failed job cgroup")?;
        }
        setup
    }
}

impl JobResources {
    /// No lazy unmount and no killing. An error retains the remaining cleanup
    /// state so a later call retries it; only complete cleanup is idempotent.
    pub fn cleanup(&self) -> Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| anyhow::anyhow!("cleanup mutex poisoned"))?;
        if state.removed {
            return Ok(());
        }
        let metadata = fs::metadata(&self.cgroup).context("inspect owned job cgroup")?;
        ensure!(
            (metadata.dev(), metadata.ino()) == self.identity,
            "job cgroup was replaced"
        );
        ensure!(
            read_pids(&self.cgroup)?.is_empty(),
            "job cgroup still contains processes"
        );
        let events = fs::read_to_string(self.cgroup.join("cgroup.events"))?;
        ensure!(
            events.lines().any(|line| line == "populated 0"),
            "job cgroup descendants still contain processes"
        );
        if let Some(id) = state.mount_id {
            ensure!(
                mount_info(&self.output)?.id == id,
                "job output mount was replaced"
            );
            unmount(&self.output)?;
            state.mount_id = None;
        }
        fs::remove_dir(&self.cgroup).context("remove empty job cgroup")?;
        state.removed = true;
        Ok(())
    }
}

fn write(dir: &Path, name: &str, value: &str) -> Result<()> {
    fs::write(dir.join(name), value).with_context(|| format!("write {}", dir.join(name).display()))
}

fn current_cgroup() -> Result<PathBuf> {
    parse_cgroup(&fs::read_to_string("/proc/self/cgroup")?)
}

fn parse_cgroup(contents: &str) -> Result<PathBuf> {
    let path = contents
        .lines()
        .find_map(|line| line.strip_prefix("0::"))
        .context("unified cgroup v2 membership is missing")?;
    let path = PathBuf::from(path);
    ensure!(
        path.is_absolute()
            && path
                .components()
                .all(|c| matches!(c, Component::RootDir | Component::Normal(_))),
        "invalid cgroup membership path"
    );
    Ok(path)
}

fn read_pids(scope: &Path) -> Result<Vec<u32>> {
    fs::read_to_string(scope.join("cgroup.procs"))?
        .split_whitespace()
        .map(|pid| pid.parse().context("invalid cgroup PID"))
        .collect()
}

fn validate_owned_pid(pid: u32, scope: &Path) -> Result<()> {
    let path = PathBuf::from(format!("/proc/{pid}"));
    ensure!(
        fs::metadata(&path)?.uid() == unsafe { libc::geteuid() },
        "refusing to migrate PID {pid} owned by another user"
    );
    ensure!(
        parse_cgroup(&fs::read_to_string(path.join("cgroup"))?)? == scope,
        "PID {pid} is outside the current container leaf"
    );
    Ok(())
}

fn unique_cgroup(parent: &Path, prefix: &str) -> Result<PathBuf> {
    let mut random = [0_u8; 16];
    let mut filled = 0;
    while filled < random.len() {
        let count = unsafe {
            libc::getrandom(
                random[filled..].as_mut_ptr().cast(),
                random.len() - filled,
                0,
            )
        };
        if count < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(error.into());
        }
        ensure!(count > 0, "getrandom returned no bytes");
        filled += count as usize;
    }
    let suffix: String = random.iter().map(|b| format!("{b:02x}")).collect();
    let path = parent.join(format!("{prefix}-{suffix}"));
    fs::create_dir(&path).with_context(|| format!("create delegated cgroup {}", path.display()))?;
    Ok(path)
}

fn cpath(path: &Path) -> Result<CString> {
    Ok(CString::new(path.as_os_str().as_bytes())?)
}

fn mount_tmpfs(output: &Path, uid: u32) -> Result<()> {
    let target = cpath(output)?;
    // tmpfs rounds size upward to pages; round down first to keep the actual
    // storage ceiling at or below the requested byte budget.
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    ensure!(page > 0, "cannot determine tmpfs page size");
    let size = MEMORY / page as u64 * page as u64;
    let options = CString::new(format!("size={size},uid={uid},gid={uid},mode=0700"))?;
    let result = unsafe {
        libc::mount(
            c"tmpfs".as_ptr(),
            target.as_ptr(),
            c"tmpfs".as_ptr(),
            libc::MS_NODEV | libc::MS_NOSUID | libc::MS_NOEXEC,
            options.as_ptr().cast(),
        )
    };
    if result != 0 {
        return Err(std::io::Error::last_os_error())
            .context("mount private budgeted tmpfs (CAP_SYS_ADMIN and mount policy required)");
    }
    if unsafe {
        libc::mount(
            std::ptr::null(),
            target.as_ptr(),
            std::ptr::null(),
            libc::MS_PRIVATE,
            std::ptr::null(),
        )
    } != 0
    {
        let error = std::io::Error::last_os_error();
        unmount(output).context("unmount after private propagation setup failed")?;
        return Err(error).context("make tmpfs mount private");
    }
    Ok(())
}

fn unmount(output: &Path) -> Result<()> {
    if unsafe { libc::umount2(cpath(output)?.as_ptr(), 0) } != 0 {
        return Err(std::io::Error::last_os_error()).context("unmount job tmpfs");
    }
    Ok(())
}

#[derive(Debug)]
struct MountInfo {
    id: u64,
    root: PathBuf,
    target: PathBuf,
    fs_type: String,
    shared: bool,
}

fn decode_mount_path(value: &str) -> Result<PathBuf> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            ensure!(i + 3 < bytes.len(), "invalid mountinfo escape");
            let digits = &bytes[i + 1..i + 4];
            ensure!(
                digits.iter().all(|d| (b'0'..=b'7').contains(d)),
                "invalid mountinfo escape"
            );
            let value = ((digits[0] - b'0') as u16) * 64
                + ((digits[1] - b'0') as u16) * 8
                + (digits[2] - b'0') as u16;
            ensure!(value <= 255, "invalid mountinfo byte");
            decoded.push(value as u8);
            i += 4;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    Ok(PathBuf::from(std::ffi::OsStr::from_bytes(&decoded)))
}

fn mounts() -> Result<Vec<MountInfo>> {
    fs::read_to_string("/proc/self/mountinfo")?
        .lines()
        .map(|line| {
            let (before, after) = line.split_once(" - ").context("invalid mountinfo")?;
            let fields: Vec<_> = before.split_whitespace().collect();
            ensure!(fields.len() >= 6, "invalid mountinfo fields");
            Ok(MountInfo {
                id: fields[0].parse()?,
                root: decode_mount_path(fields[3])?,
                target: decode_mount_path(fields[4])?,
                fs_type: after
                    .split_whitespace()
                    .next()
                    .context("missing mount filesystem")?
                    .to_owned(),
                shared: fields[6..].iter().any(|f| f.starts_with("shared:")),
            })
        })
        .collect()
}

fn active_mount_id(target: &Path) -> Result<u64> {
    let path = CString::new(target.as_os_str().as_bytes())?;
    let mut stat: libc::statx = unsafe { std::mem::zeroed() };
    let result = unsafe {
        libc::statx(
            libc::AT_FDCWD,
            path.as_ptr(),
            0,
            libc::STATX_MNT_ID,
            &mut stat,
        )
    };
    ensure!(
        result == 0,
        "mount identity lookup failed: {}",
        std::io::Error::last_os_error()
    );
    ensure!(
        stat.stx_mask & libc::STATX_MNT_ID != 0,
        "kernel mount identity unavailable"
    );
    Ok(stat.stx_mnt_id)
}
fn mount_info(target: &Path) -> Result<MountInfo> {
    let id = active_mount_id(target)?;
    mounts()?
        .into_iter()
        .find(|m| m.id == id && m.target == target)
        .with_context(|| format!("no active mount at {}", target.display()))
}
fn ensure_private_ancestor(target: &Path) -> Result<()> {
    let id = active_mount_id(target)?;
    let ancestor = mounts()?
        .into_iter()
        .find(|m| m.id == id)
        .context("no active mount ancestor")?;
    ensure!(
        !ancestor.shared,
        "output ancestor is shared; supply a private container mount namespace"
    );
    Ok(())
}

impl Drop for ResourceDomain {
    fn drop(&mut self) {
        if Arc::strong_count(&self.root) == 1 {
            let _ = fs::remove_dir(self.root.as_ref());
        }
    }
}
impl Drop for JobResources {
    fn drop(&mut self) {
        if Arc::strong_count(&self.domain) == 1 {
            let _ = fs::remove_dir(self.domain.as_ref());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ancestor_scope_requires_exact_verified_supervisor_migration() {
        assert!(!scope_allowed(
            Path::new("/sys/fs/cgroup"),
            Path::new("/sys/fs/cgroup/convt-container.scope"),
            Path::new("/tenant.slice"),
            false
        ));
        assert!(!scope_allowed(
            Path::new("/sys/fs/cgroup"),
            Path::new("/sys/fs/cgroup/convt-supervisor-unknown"),
            Path::new("/container.scope"),
            false
        ));
        assert!(scope_allowed(
            Path::new("/sys/fs/cgroup"),
            Path::new("/sys/fs/cgroup/convt-supervisor-owned"),
            Path::new("/container.scope"),
            true
        ));
        assert!(scope_allowed(
            Path::new("/sys/fs/cgroup/job"),
            Path::new("/sys/fs/cgroup"),
            Path::new("/container.scope"),
            false
        ));
    }
    #[test]
    fn active_mount_uses_kernel_identity() {
        let root = mount_info(Path::new("/")).unwrap();
        assert_eq!(root.id, active_mount_id(Path::new("/")).unwrap());
        assert_eq!(root.target, Path::new("/"));
    }
    #[test]
    fn mount_paths_decode_spaces_and_refuse_bad_escapes() {
        assert_eq!(
            decode_mount_path(r"/a\040b").unwrap(),
            PathBuf::from("/a b")
        );
        assert!(decode_mount_path(r"/a\999b").is_err());
    }
}
