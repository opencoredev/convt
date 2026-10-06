//! Linux per-job confinement for hosts without a container daemon.
use anyhow::{Context, bail, ensure};
use std::{
    ffi::CString,
    fs,
    os::unix::{
        ffi::OsStrExt,
        fs::{FileTypeExt, MetadataExt, PermissionsExt},
    },
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::{Child, Command};

const READ: u64 = (1 << 0) | (1 << 2) | (1 << 3);
const FS_ALL: u64 = (1 << 16) - 1;
const ABI_REQUIRED: i64 = 6;

#[derive(Clone)]
pub struct ProcessSandbox {
    pub template: PathBuf,
    pub executable: PathBuf,
    pub jobs: PathBuf,
    pub allow_unsafe: bool,
    resources: Option<crate::resource_domain::ResourceDomain>,
    active: std::sync::Arc<
        tokio::sync::Mutex<
            std::collections::HashMap<String, std::sync::Arc<tokio::sync::Mutex<ActiveJob>>>,
        >,
    >,
}
#[derive(Clone, Copy)]
pub struct Limits {
    pub cpu: u64,
    pub memory: u64,
    pub file: u64,
    pub processes: u64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            cpu: 600,
            memory: 4_000_000_000,
            file: 2_000_000_000,
            processes: 128,
        }
    }
}
fn cpath(path: &Path) -> anyhow::Result<CString> {
    Ok(CString::new(path.as_os_str().as_bytes())?)
}
fn checked(result: i64) -> anyhow::Result<i64> {
    if result < 0 {
        Err(std::io::Error::last_os_error().into())
    } else {
        Ok(result)
    }
}
pub fn landlock_abi() -> i64 {
    unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            std::ptr::null::<u8>(),
            0,
            1,
        )
    }
}

#[repr(C)]
struct Ruleset {
    fs: u64,
    net: u64,
    scopes: u64,
}
#[repr(C, packed)]
struct PathRule {
    access: u64,
    parent: i32,
}
fn landlock(input: &Path, output: &Path) -> anyhow::Result<()> {
    ensure!(
        landlock_abi() >= ABI_REQUIRED,
        "Landlock ABI 6 or newer is required"
    );
    let rules = Ruleset {
        fs: FS_ALL,
        net: 3,
        scopes: 3,
    };
    let fd = checked(unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            &rules,
            size_of::<Ruleset>(),
            0,
        )
    })? as i32;
    let result =
        (|| {
            for (path, rights) in [
                (Path::new("/opt/convt"), READ),
                (Path::new("/usr"), READ),
                (Path::new("/lib"), READ),
                (Path::new("/lib64"), READ),
                (Path::new("/bin"), READ),
                (Path::new("/etc"), READ & !(1 << 0)),
                (Path::new("/dev/null"), (1 << 1) | (1 << 2) | (1 << 15)),
                (Path::new("/dev/urandom"), 1 << 2),
                (Path::new("/dev/random"), 1 << 2),
                (input, 1 << 2),
                (output, FS_ALL & !(1 << 0)),
            ] {
                if !path.exists() {
                    continue;
                }
                let path_fd = checked(unsafe {
                    libc::open(cpath(path)?.as_ptr(), libc::O_PATH | libc::O_CLOEXEC)
                } as i64)? as i32;
                let rule = PathRule {
                    access: rights,
                    parent: path_fd,
                };
                let added = unsafe { libc::syscall(libc::SYS_landlock_add_rule, fd, 1, &rule, 0) };
                unsafe {
                    libc::close(path_fd);
                }
                checked(added).with_context(|| format!("Landlock rule {}", path.display()))?;
            }
            checked(unsafe { libc::syscall(libc::SYS_landlock_restrict_self, fd, 0) })?;
            Ok(())
        })();
    unsafe {
        libc::close(fd);
    }
    result
}

fn statement(code: u16, k: u32) -> libc::sock_filter {
    libc::sock_filter {
        code,
        jt: 0,
        jf: 0,
        k,
    }
}
fn jump(k: u32, jt: u8, jf: u8) -> libc::sock_filter {
    libc::sock_filter {
        code: 0x15,
        jt,
        jf,
        k,
    }
}
fn seccomp() -> anyhow::Result<()> {
    #[cfg(target_arch = "x86_64")]
    let arch = 0xc000003e;
    #[cfg(target_arch = "aarch64")]
    let arch = 0xc00000b7;
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    compile_error!("The process sandbox supports Linux x86_64 and aarch64");
    let deny = 0x00050000 | libc::EPERM as u32;
    let mut filter = vec![
        statement(0x20, 4),
        jump(arch, 1, 0),
        statement(0x06, 0x80000000),
        statement(0x20, 0),
    ];
    // Reject the alternate x32 syscall ABI rather than bypassing native rules.
    #[cfg(target_arch = "x86_64")]
    filter.extend([
        libc::sock_filter {
            code: 0x45,
            jt: 0,
            jf: 1,
            k: 0x40000000,
        },
        statement(0x06, 0x80000000),
    ]);
    for syscall in [libc::SYS_socket, libc::SYS_socketpair] {
        filter.extend([
            jump(syscall as u32, 0, 4),
            statement(0x20, 16),
            jump(libc::AF_UNIX as u32, 1, 0),
            statement(0x06, deny),
            statement(0x06, 0x7fff0000),
        ]);
    }
    filter.extend([
        jump(libc::SYS_clone3 as u32, 0, 1),
        statement(0x06, 0x00050000 | libc::ENOSYS as u32),
    ]);
    let flags = (libc::CLONE_NEWUSER
        | libc::CLONE_NEWNS
        | libc::CLONE_NEWPID
        | libc::CLONE_NEWNET
        | libc::CLONE_NEWIPC
        | libc::CLONE_NEWUTS
        | libc::CLONE_NEWCGROUP
        | libc::CLONE_PARENT
        | libc::CLONE_UNTRACED) as u32;
    filter.extend([
        jump(libc::SYS_clone as u32, 0, 4),
        statement(0x20, 16),
        libc::sock_filter {
            code: 0x45,
            jt: 0,
            jf: 1,
            k: flags,
        },
        statement(0x06, deny),
        statement(0x20, 0),
    ]);
    for syscall in [
        libc::SYS_setpgid,
        libc::SYS_setsid,
        libc::SYS_unshare,
        libc::SYS_setns,
        libc::SYS_mount,
        libc::SYS_umount2,
        libc::SYS_pivot_root,
        libc::SYS_chroot,
        libc::SYS_ptrace,
        libc::SYS_process_vm_readv,
        libc::SYS_process_vm_writev,
        libc::SYS_pidfd_getfd,
        libc::SYS_open_by_handle_at,
        libc::SYS_bpf,
        libc::SYS_perf_event_open,
        libc::SYS_userfaultfd,
        libc::SYS_io_uring_setup,
        libc::SYS_io_uring_enter,
        libc::SYS_io_uring_register,
        libc::SYS_keyctl,
        libc::SYS_add_key,
        libc::SYS_request_key,
        libc::SYS_reboot,
        libc::SYS_kexec_load,
    ] {
        filter.extend([jump(syscall as u32, 0, 1), statement(0x06, deny)]);
    }
    filter.push(statement(0x06, 0x7fff0000));
    let program = libc::sock_fprog {
        len: filter.len() as u16,
        filter: filter.as_mut_ptr(),
    };
    checked(unsafe { libc::syscall(libc::SYS_seccomp, 1, 0, &program) })?;
    Ok(())
}
fn rlimit(resource: libc::__rlimit_resource_t, limit: u64) -> anyhow::Result<()> {
    let value = libc::rlimit {
        rlim_cur: limit,
        rlim_max: limit,
    };
    checked(unsafe { libc::setrlimit(resource, &value) } as i64)?;
    Ok(())
}

/// A fresh exec reaches this before any Tokio runtime or worker credentials.
pub fn child_main(args: &[String]) -> anyhow::Result<()> {
    ensure!(args.len() == 10, "invalid sandbox child arguments");
    ensure!(
        std::env::vars_os().next().is_none(),
        "child environment must be empty"
    );
    let root = cpath(Path::new(&args[0]))?;
    let uid: u32 = args[1].parse()?;
    ensure!(uid >= 10000, "unprivileged job uid required");
    let input = Path::new(&args[2]);
    let target = &args[3];
    let limits = Limits {
        cpu: args[5].parse()?,
        memory: args[6].parse()?,
        file: args[7].parse()?,
        processes: args[8].parse()?,
    };
    // No inherited descriptors or cwd can point outside the private root.
    checked(unsafe { libc::syscall(libc::SYS_close_range, 3_u32, u32::MAX, 0) })?;
    let supervisor_fd =
        checked(unsafe { libc::syscall(libc::SYS_pidfd_open, libc::getppid(), 0) })? as i32;
    checked(unsafe { libc::fcntl(supervisor_fd, libc::F_SETFD, libc::FD_CLOEXEC) } as i64)?;
    let cgroup_file = if args[9].is_empty() {
        None
    } else {
        Some(fs::OpenOptions::new().write(true).open(&args[9])?)
    };
    checked(unsafe { libc::chroot(root.as_ptr()) } as i64)?;
    checked(unsafe { libc::chdir(c"/output".as_ptr()) } as i64)?;
    let allow_unsafe = args[4] == "unsafe";
    use std::os::fd::AsRawFd;
    let guardian = watchdog(
        supervisor_fd,
        cgroup_file.as_ref().map_or(-1, AsRawFd::as_raw_fd),
        allow_unsafe,
    )?;
    let aggregate = cgroup_file.is_some();
    if let Some(mut file) = cgroup_file {
        use std::io::Write;
        file.write_all(format!("{}\n", unsafe { libc::getpid() }).as_bytes())
            .context("enroll child cgroup")?;
    }
    ensure!(
        aggregate || allow_unsafe,
        "aggregate resource domain is required"
    );
    checked(unsafe { libc::setgroups(0, std::ptr::null()) } as i64)?;
    checked(unsafe { libc::setresgid(uid, uid, uid) } as i64)?;
    checked(unsafe { libc::setresuid(uid, uid, uid) } as i64)?;
    checked(unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0) } as i64)?;
    checked(unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } as i64)?;
    #[repr(C)]
    struct CapHeader {
        version: u32,
        pid: i32,
    }
    #[repr(C)]
    struct CapData {
        effective: u32,
        permitted: u32,
        inheritable: u32,
    }
    let header = CapHeader {
        version: 0x20080522,
        pid: 0,
    };
    let caps = [
        CapData {
            effective: 0,
            permitted: 0,
            inheritable: 0,
        },
        CapData {
            effective: 0,
            permitted: 0,
            inheritable: 0,
        },
    ];
    checked(unsafe { libc::syscall(libc::SYS_capset, &header, &caps) })?;
    checked(unsafe {
        libc::prctl(
            libc::PR_CAP_AMBIENT,
            libc::PR_CAP_AMBIENT_CLEAR_ALL,
            0,
            0,
            0,
        )
    } as i64)?;
    rlimit(libc::RLIMIT_CORE, 0)?;
    rlimit(libc::RLIMIT_CPU, limits.cpu)?;
    rlimit(libc::RLIMIT_AS, limits.memory)?;
    rlimit(libc::RLIMIT_FSIZE, limits.file)?;
    rlimit(libc::RLIMIT_NPROC, limits.processes)?;
    rlimit(libc::RLIMIT_NOFILE, 256)?;
    let ll = landlock(input, Path::new("/output"));
    if !allow_unsafe {
        ll.as_ref().map_err(|e| anyhow::anyhow!("Landlock: {e}"))?;
    }
    let sc = seccomp();
    if !allow_unsafe {
        sc.as_ref().map_err(|e| anyhow::anyhow!("seccomp: {e}"))?;
    }
    let protections = serde_json::json!({"aggregate_cgroup":aggregate,"bounded_tmpfs":aggregate,"landlock":ll.is_ok(),"seccomp":sc.is_ok(),"landlock_abi":landlock_abi(),"empty_environment":true,"no_new_privs":unsafe{libc::prctl(libc::PR_GET_NO_NEW_PRIVS,0,0,0,0)} == 1,"capabilities_cleared":true,"supervisor_watchdog":true,"guardian_protected":unsafe{libc::kill(guardian,libc::SIGTERM)} < 0,"uid":unsafe{libc::getuid()},"chroot":true,"proc_mounted":Path::new("/proc").exists(),"unsafe_override":allow_unsafe});
    if let Some(probe) = target.strip_prefix("probe:") {
        return probe_child(probe, protections);
    }
    convt_engines::use_cloud_supervisor(Path::new("/opt/convt")).map_err(anyhow::Error::msg)?;
    tempfile::env::override_temp_dir(Path::new("/output/scratch"))
        .map_err(|_| anyhow::anyhow!("temporary directory override failed"))?;
    let registry = convt_engines::default_registry();
    if target == "formats" {
        let formats: Vec<_> = convt_core::FORMATS.iter().map(|f| serde_json::json!({"id":f.id,"name":f.name,"extensions":f.extensions,"category":format!("{:?}",f.category).to_lowercase(),"mime":f.mime,"targets":registry.targets(f).iter().map(|t|t.id).collect::<Vec<_>>()})).collect();
        println!("{}", serde_json::to_string(&formats)?);
        return Ok(());
    }
    let to = convt_core::format_by_id(target).ok_or_else(|| anyhow::anyhow!("invalid target"))?;
    let job = convt_core::Job {
        output: convt_core::Output::Dir(PathBuf::from("/output/files")),
        ..convt_core::Job::new(input.to_path_buf(), to)
    };
    let results =
        convt_core::run_batch(&registry, &[job], 1, &convt_core::Cancel::new(), &|_, _| {});
    results.into_iter().next().unwrap()?;
    Ok(())
}

// A confined sibling keeps the group bounded even if the parent worker dies.
// pidfd avoids guessing from reused process ids. It is never inherited by engines.
fn watchdog(supervisor_fd: i32, cgroup_fd: i32, allow_unsafe: bool) -> anyhow::Result<i32> {
    let mut ready = [0; 2];
    checked(unsafe { libc::pipe2(ready.as_mut_ptr(), libc::O_CLOEXEC) } as i64)?;
    let pid = checked(unsafe { libc::fork() } as i64)?;
    if pid == 0 {
        unsafe {
            libc::close(ready[0]);
            if cgroup_fd >= 0 {
                libc::close(cgroup_fd);
            }
        }
        // Keep the guardian outside the job uid and Landlock signal domain: a
        // compromised converter must not be able to stop its deadline monitor.
        let setup = (|| -> anyhow::Result<()> {
            checked(unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } as i64)?;
            #[cfg(target_arch = "x86_64")]
            let arch = 0xc000003e;
            #[cfg(target_arch = "aarch64")]
            let arch = 0xc00000b7;
            let mut filter = vec![
                statement(0x20, 4),
                jump(arch, 1, 0),
                statement(0x06, 0x80000000),
                statement(0x20, 0),
            ];
            for syscall in [
                libc::SYS_write,
                libc::SYS_close,
                libc::SYS_poll,
                libc::SYS_ppoll,
                libc::SYS_clock_gettime,
                libc::SYS_getpgrp,
                libc::SYS_kill,
                libc::SYS_exit,
                libc::SYS_exit_group,
                libc::SYS_rt_sigreturn,
            ] {
                filter.extend([jump(syscall as u32, 0, 1), statement(0x06, 0x7fff0000)]);
            }
            filter.push(statement(0x06, 0x80000000));
            let program = libc::sock_fprog {
                len: filter.len() as u16,
                filter: filter.as_mut_ptr(),
            };
            checked(unsafe { libc::syscall(libc::SYS_seccomp, 1, 0, &program) })?;
            Ok(())
        })();
        if setup.is_err() && !allow_unsafe {
            unsafe {
                libc::kill(-libc::getpgrp(), libc::SIGKILL);
                libc::_exit(1);
            }
        }
        unsafe {
            libc::write(ready[1], c"1".as_ptr().cast(), 1);
            libc::close(ready[1]);
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(600);
        let mut event = libc::pollfd {
            fd: supervisor_fd,
            events: libc::POLLIN,
            revents: 0,
        };
        loop {
            let result = unsafe { libc::poll(&mut event, 1, 100) };
            if result != 0 || std::time::Instant::now() >= deadline {
                unsafe {
                    libc::kill(-libc::getpgrp(), libc::SIGKILL);
                    libc::_exit(1);
                }
            }
        }
    }
    unsafe {
        libc::close(ready[1]);
        libc::close(supervisor_fd);
    }
    let mut event = libc::pollfd {
        fd: ready[0],
        events: libc::POLLIN,
        revents: 0,
    };
    let mut byte = 0_u8;
    let received = unsafe {
        libc::poll(&mut event, 1, 1000) > 0
            && libc::read(ready[0], (&mut byte as *mut u8).cast(), 1) == 1
    };
    unsafe {
        libc::close(ready[0]);
    }
    ensure!(received, "sandbox guardian failed startup");
    Ok(pid as i32)
}

fn probe_child(probe: &str, mut protections: serde_json::Value) -> anyhow::Result<()> {
    match probe {
        "isolation" => {
            let denied = fs::read("/proc/1/environ").is_err()
                && fs::read("/parent-secret").is_err()
                && fs::write("/etc/escape", b"x").is_err();
            let tcp = unsafe { libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0) };
            let udp = unsafe { libc::socket(libc::AF_INET6, libc::SOCK_DGRAM, 0) };
            if tcp >= 0 {
                unsafe {
                    libc::close(tcp);
                }
            }
            if udp >= 0 {
                unsafe {
                    libc::close(udp);
                }
            }
            let group_escape = unsafe { libc::setpgid(0, 0) } < 0 && unsafe { libc::setsid() } < 0;
            fs::write("/output/probe-write", b"allowed")?;
            protections["escape_attempts_denied"] =
                serde_json::json!(denied && tcp < 0 && udp < 0 && group_escape);
            println!("{protections}");
        }
        "aggregate-memory" => {
            checked(unsafe { libc::fork() } as i64)?;
            let bytes = 2_200_000_000;
            let allocation = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    bytes,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                    -1,
                    0,
                )
            };
            ensure!(
                allocation != libc::MAP_FAILED,
                "aggregate allocation failed before touching pages"
            );
            for offset in (0..bytes).step_by(4096) {
                unsafe {
                    std::ptr::write_volatile((allocation as *mut u8).add(offset), 1);
                }
            }
            loop {
                unsafe {
                    libc::pause();
                }
            }
        }
        "aggregate-cpu" => {
            for _ in 0..3 {
                if checked(unsafe { libc::fork() } as i64)? == 0 {
                    break;
                }
            }
            loop {
                std::hint::black_box(1_u64.wrapping_mul(3));
            }
        }
        "unlinked-storage" => {
            use std::io::Write;
            let mut files = Vec::new();
            let mut bytes = 0_u64;
            for index in 0..3 {
                let name = format!("/output/unlinked-{index}");
                let mut file = fs::File::create(&name)?;
                fs::remove_file(&name)?;
                for _ in 0..(1_800_000_000 / 65536) {
                    match file.write_all(&[1; 65536]) {
                        Ok(()) => bytes += 65536,
                        Err(error) if error.raw_os_error() == Some(libc::ENOSPC) => {
                            ensure!(
                                bytes > 3_900_000_000 && bytes < 4_000_000_000,
                                "invalid tmpfs byte boundary"
                            );
                            println!("PASS unlinked storage hard cap: {bytes} bytes, ENOSPC");
                            return Ok(());
                        }
                        Err(error) => return Err(error.into()),
                    }
                }
                files.push(file);
            }
            bail!("unlinked files escaped scratch limit");
        }
        "office" => {
            let output = std::process::Command::new("/usr/lib/libreoffice/program/soffice.bin")
                .args([
                    "-env:UserInstallation=file:///output/scratch/profile",
                    "--headless",
                    "--norestore",
                    "--convert-to",
                    "pdf",
                    "--outdir",
                    "/output/files",
                    "/input.txt",
                ])
                .output()?;
            println!(
                "status={} stdout={} stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        "fork" => {
            let mut count = 0;
            loop {
                let pid = unsafe { libc::fork() };
                if pid < 0 {
                    break;
                }
                if pid == 0 {
                    loop {
                        unsafe {
                            libc::pause();
                        }
                    }
                }
                count += 1;
                ensure!(count <= 128, "fork limit escaped");
            }
            ensure!(count > 0 && count < 128, "invalid process limit probe");
            println!("PASS fork limit: {count} children");
        }
        "memory" => {
            let allocation = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    8_000_000_000,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                    -1,
                    0,
                )
            };
            ensure!(allocation == libc::MAP_FAILED, "memory limit escaped");
            println!("PASS memory limit");
        }
        "fsize" => {
            use std::io::Write;
            let mut f = fs::File::create("/output/oversized")?;
            for _ in 0..2048 {
                f.write_all(&[0; 4096])?;
            }
            bail!("file limit escaped");
        }
        "cpu" => loop {
            std::hint::black_box(1_u64.wrapping_mul(3));
        },
        "tree" => {
            if unsafe { libc::fork() } == 0 {
                loop {
                    unsafe {
                        libc::pause();
                    }
                }
            }
            loop {
                unsafe {
                    libc::pause();
                }
            }
        }
        _ => bail!("unknown sandbox probe"),
    }
    Ok(())
}

impl ProcessSandbox {
    pub fn from_env() -> anyhow::Result<Self> {
        ensure!(
            unsafe { libc::geteuid() } == 0,
            "process sandbox requires a root supervisor, SETUID/SETGID and SYS_CHROOT"
        );
        let template = PathBuf::from(
            std::env::var("CONVT_SANDBOX_TEMPLATE")
                .unwrap_or_else(|_| "/srv/convt-template".into()),
        );
        validate_template(&template)?;
        let jobs = PathBuf::from(
            std::env::var("CONVT_SANDBOX_JOBS").unwrap_or_else(|_| "/srv/convt-jobs".into()),
        );
        fs::create_dir_all(&jobs)?;
        fs::set_permissions(&jobs, fs::Permissions::from_mode(0o700))?;
        checked(unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1) } as i64)?;
        let allow_unsafe = std::env::var("CONVT_SANDBOX_ALLOW_UNSAFE").as_deref() == Ok("1");
        let resources = match crate::resource_domain::ResourceDomain::from_env() {
            Ok(domain) => Some(domain),
            Err(error) if allow_unsafe => {
                tracing::warn!(%error,"unsafe override disables aggregate resource domain");
                None
            }
            Err(error) => return Err(error.context("kernel resource gate")),
        };
        Ok(Self {
            resources,
            template,
            executable: std::env::current_exe()?,
            jobs,
            active: Default::default(),
            allow_unsafe,
        })
    }
    pub fn prepare(&self, input: &Path, extension: &str) -> anyhow::Result<JobRoot> {
        ensure!(
            extension.bytes().all(|b| b.is_ascii_alphanumeric()),
            "invalid extension"
        );
        let root = tempfile::Builder::new()
            .prefix("job-")
            .tempdir_in(&self.jobs)?;
        fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755))?;
        link_tree(&self.template, root.path())?;
        let uid = allocate_uid(&self.jobs)?;
        fs::create_dir(root.path().join("output"))?;
        let resources = self
            .resources
            .as_ref()
            .map(|domain| domain.create(uid, &root.path().join("output")))
            .transpose()?;
        let root = JobRoot {
            root: std::mem::ManuallyDrop::new(root),
            uid,
            input: format!("/input.{extension}"),
            resources,
        };
        for name in [
            "output",
            "output/scratch",
            "output/files",
            "output/scratch/font-cache",
        ] {
            let path = root.root.path().join(name);
            fs::create_dir_all(&path)?;
            checked(unsafe { libc::chown(cpath(&path)?.as_ptr(), uid, uid) } as i64)?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
        }
        for name in ["etc/passwd", "etc/group"] {
            let path = root.root.path().join(name);
            if fs::symlink_metadata(&path).is_ok() {
                fs::remove_file(&path)?;
            }
        }
        fs::write(
            root.root.path().join("etc/passwd"),
            format!("root:x:0:0:root:/root:/bin/sh\nconvt:x:{uid}:{uid}:job:/output:/bin/sh\n"),
        )?;
        fs::write(
            root.root.path().join("etc/group"),
            format!("root:x:0:\nconvt:x:{uid}:\n"),
        )?;
        std::os::unix::fs::symlink("output/scratch", root.root.path().join("tmp"))?;
        let dest = root.root.path().join(format!("input.{extension}"));
        fs::copy(input, &dest)?;
        fs::set_permissions(&dest, fs::Permissions::from_mode(0o444))?;
        Ok(root)
    }
    pub fn spawn(
        &self,
        root: &JobRoot,
        target: &str,
        limits: Limits,
        capture: bool,
    ) -> anyhow::Result<GroupProcess> {
        let mut command = Command::new(&self.executable);
        command
            .args([
                "--sandbox-child",
                root.root.path().to_str().context("invalid job path")?,
                &root.uid.to_string(),
                &root.input,
                target,
                if self.allow_unsafe {
                    "unsafe"
                } else {
                    "enforced"
                },
                &limits.cpu.to_string(),
                &limits.memory.to_string(),
                &limits.file.to_string(),
                &limits.processes.to_string(),
                root.resources
                    .as_ref()
                    .map_or("", |resource| resource.cgroup_procs.to_str().unwrap()),
            ])
            .env_clear()
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .stdout(if capture {
                std::process::Stdio::piped()
            } else {
                std::process::Stdio::null()
            })
            .kill_on_drop(true);
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::syscall(libc::SYS_close_range, 3_u32, u32::MAX, 4_u32) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn()?;
        let pid = child.id().context("child pid missing")? as i32;
        Ok(GroupProcess {
            child,
            pid,
            armed: true,
            status: None,
            group_clean: false,
            budget: None,
        })
    }
    pub async fn start(
        &self,
        name: &str,
        root: JobRoot,
        target: &str,
    ) -> anyhow::Result<std::sync::Arc<tokio::sync::Mutex<ActiveJob>>> {
        let mut process = self.spawn(&root, target, Limits::default(), false)?;
        process.budget = None;
        let active = std::sync::Arc::new(tokio::sync::Mutex::new(ActiveJob { root, process }));
        self.active.lock().await.insert(name.into(), active.clone());
        Ok(active)
    }
    pub async fn stop(&self, name: &str) -> anyhow::Result<()> {
        let active = self.active.lock().await.get(name).cloned();
        if let Some(active) = active {
            {
                let mut job = active.lock().await;
                job.process.stop().await?;
                job.root.cleanup()?;
            }
            self.active.lock().await.remove(name);
        }
        Ok(())
    }
    pub async fn office_probe(&self) -> anyhow::Result<()> {
        let input = tempfile::NamedTempFile::new_in(&self.jobs)?;
        fs::write(input.path(), b"A real document conversion probe.\n")?;
        let root = self.prepare(input.path(), "txt")?;
        let mut child = self.spawn(&root, "probe:office", Limits::default(), true)?;
        let report = child.wait_report(Duration::from_secs(30)).await?;
        println!("{} {} {}", report.0, report.1, report.2);
        Ok(())
    }
    pub async fn formats(&self) -> anyhow::Result<serde_json::Value> {
        let input = tempfile::NamedTempFile::new_in(&self.jobs)?;
        let root = self.prepare(input.path(), "txt")?;
        let mut child = self.spawn(&root, "formats", Limits::default(), true)?;
        let (status, output, error) = child.wait_report(Duration::from_secs(45)).await?;
        ensure!(status.success(), "sandbox engine discovery failed: {error}");
        Ok(serde_json::from_str(&output)?)
    }
    pub async fn verify_resources(&self) -> anyhow::Result<()> {
        for (probe, success) in [
            ("fork", true),
            ("memory", true),
            ("fsize", false),
            ("cpu", false),
        ] {
            let input = tempfile::NamedTempFile::new_in(&self.jobs)?;
            let root = self.prepare(input.path(), "txt")?;
            let limits = Limits {
                cpu: 1,
                file: 1_000_000,
                processes: 12,
                ..Limits::default()
            };
            let mut child = self.spawn(&root, &format!("probe:{probe}"), limits, true)?;
            let (status, output, error) = child.wait_report(Duration::from_secs(5)).await?;
            ensure!(
                status.success() == success,
                "{probe} probe failed: {status} {output} {error}"
            );
            if !success {
                use std::os::unix::process::ExitStatusExt;
                ensure!(
                    status.signal()
                        == Some(if probe == "cpu" {
                            libc::SIGKILL
                        } else {
                            libc::SIGXFSZ
                        }),
                    "unexpected resource probe termination"
                );
            }
            println!("PASS {probe} limit");
        }
        if self.resources.is_some() {
            for probe in ["aggregate-memory", "aggregate-cpu", "unlinked-storage"] {
                let input = tempfile::NamedTempFile::new_in(&self.jobs)?;
                let root = self.prepare(input.path(), "txt")?;
                let group = root
                    .resources
                    .as_ref()
                    .unwrap()
                    .cgroup_procs
                    .parent()
                    .unwrap();
                if probe == "unlinked-storage" {
                    fs::write(group.join("memory.max"), b"6000000000")?;
                }
                let mut child =
                    self.spawn(&root, &format!("probe:{probe}"), Limits::default(), true)?;
                let started = std::time::Instant::now();
                let (status, output, error) = child
                    .wait_report(Duration::from_secs(if probe == "aggregate-cpu" {
                        3
                    } else {
                        30
                    }))
                    .await?;
                let stats = |name: &str, key: &str| -> anyhow::Result<u64> {
                    let contents = fs::read_to_string(group.join(name))?;
                    Ok(contents
                        .lines()
                        .find_map(|line| line.strip_prefix(&format!("{key} ")))
                        .context("missing cgroup statistic")?
                        .parse()?)
                };
                match probe {
                    "aggregate-memory" => ensure!(
                        !status.success() && stats("memory.events", "oom_kill")? > 0,
                        "aggregate memory cap did not kill group: {error}"
                    ),
                    "aggregate-cpu" => {
                        let usage = stats("cpu.stat", "usage_usec")?;
                        ensure!(
                            usage > 1_000_000
                                && usage
                                    < (started.elapsed().as_micros() as u64) * 21 / 10 + 200_000
                                && stats("cpu.stat", "nr_throttled")? > 0,
                            "aggregate CPU cap escaped: {usage}"
                        );
                    }
                    _ => ensure!(
                        status.success() && output.contains("ENOSPC"),
                        "unlinked storage probe failed: {status} {output} {error}"
                    ),
                }
                root.cleanup()?;
                println!("PASS {probe} kernel bound");
            }
        }
        for wall in [false, true] {
            let input = tempfile::NamedTempFile::new_in(&self.jobs)?;
            let root = self.prepare(input.path(), "txt")?;
            let mut child = self.spawn(&root, "probe:tree", Limits::default(), true)?;
            if wall {
                child.wait_report(Duration::from_millis(250)).await?;
            } else {
                tokio::time::sleep(Duration::from_millis(250)).await;
                child.stop().await?;
            }
            ensure!(
                unsafe { libc::kill(-child.pid(), 0) } < 0,
                "process group survived cancellation"
            );
            println!(
                "PASS {} kills whole tree",
                if wall { "wall timeout" } else { "cancellation" }
            );
        }
        Ok(())
    }
    pub async fn gate(&self) -> anyhow::Result<serde_json::Value> {
        let input = tempfile::NamedTempFile::new_in(&self.jobs)?;
        let root = self.prepare(input.path(), "txt")?;
        let mut child = self.spawn(&root, "probe:isolation", Limits::default(), true)?;
        let report = child.wait_report(Duration::from_secs(15)).await?;
        ensure!(
            report.0.success(),
            "sandbox startup probe failed: {}",
            report.2
        );
        let value: serde_json::Value = serde_json::from_str(&report.1)?;
        if !self.allow_unsafe {
            ensure!(
                value["aggregate_cgroup"] == true
                    && value["bounded_tmpfs"] == true
                    && value["guardian_protected"] == true
                    && value["landlock"] == true
                    && value["seccomp"] == true
                    && value["escape_attempts_denied"] == true
                    && value["proc_mounted"] == false,
                "sandbox capability gate failed: {value}"
            );
        }
        Ok(value)
    }
}

pub struct ActiveJob {
    pub root: JobRoot,
    pub process: GroupProcess,
}

pub struct JobRoot {
    pub root: std::mem::ManuallyDrop<tempfile::TempDir>,
    pub resources: Option<crate::resource_domain::JobResources>,
    pub uid: u32,
    pub input: String,
}
impl JobRoot {
    pub fn cleanup(&self) -> anyhow::Result<()> {
        if let Some(resources) = &self.resources {
            resources.cleanup()?;
        }
        Ok(())
    }
}
impl Drop for JobRoot {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            tracing::error!(%error,"job root retained after failed resource cleanup");
            return;
        }
        unsafe {
            std::mem::ManuallyDrop::drop(&mut self.root);
        }
    }
}
fn allocate_uid(jobs: &Path) -> anyhow::Result<u32> {
    let locks = jobs.join("uids");
    fs::create_dir_all(&locks)?;
    for _ in 0..1000 {
        let mut value = 0_u32;
        checked(
            unsafe { libc::getrandom((&mut value as *mut u32).cast(), size_of::<u32>(), 0) } as i64,
        )?;
        let uid = 10000 + value % 2_000_000_000;
        match fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(locks.join(uid.to_string()))
        {
            Ok(_) => return Ok(uid),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.into()),
        }
    }
    bail!("job uid allocation exhausted")
}
fn validate_template(path: &Path) -> anyhow::Result<()> {
    ensure!(
        path.is_absolute() && path.is_dir(),
        "sandbox template must be an absolute directory"
    );
    for ancestor in path.ancestors() {
        let meta = fs::symlink_metadata(ancestor)?;
        ensure!(
            !meta.file_type().is_symlink() && meta.uid() == 0 && meta.mode() & 0o022 == 0,
            "sandbox template ancestry must be root-owned and immutable"
        );
    }
    ensure!(
        fs::symlink_metadata(path.join("etc"))?.is_dir()
            && fs::symlink_metadata(path.join("dev"))?.is_dir(),
        "template etc and dev must be real directories"
    );
    ensure!(
        !path.join("proc").exists() && !path.join("tmp").exists() && !path.join("output").exists(),
        "template cannot include proc, tmp or job output"
    );
    Ok(())
}
fn link_tree(from: &Path, to: &Path) -> anyhow::Result<()> {
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let src = entry.path();
        let dst = to.join(entry.file_name());
        let meta = fs::symlink_metadata(&src)?;
        ensure!(
            meta.uid() == 0
                && (meta.file_type().is_symlink()
                    || meta.file_type().is_char_device()
                    || meta.mode() & 0o022 == 0),
            "mutable template entry {}",
            src.display()
        );
        if meta.is_dir() {
            fs::create_dir(&dst)?;
            fs::set_permissions(&dst, fs::Permissions::from_mode(meta.mode() & 0o777))?;
            link_tree(&src, &dst)?;
        } else if meta.is_file() {
            fs::hard_link(&src, &dst).or_else(|_| fs::copy(&src, &dst).map(|_| ()))?;
        } else if meta.file_type().is_symlink() {
            std::os::unix::fs::symlink(fs::read_link(&src)?, &dst)?;
        } else {
            ensure!(
                src.ends_with("dev/null")
                    || src.ends_with("dev/urandom")
                    || src.ends_with("dev/random"),
                "unexpected special file"
            );
            let minor = if src.ends_with("dev/null") {
                3
            } else if src.ends_with("dev/urandom") {
                9
            } else {
                8
            };
            ensure!(
                meta.file_type().is_char_device()
                    && libc::major(meta.rdev()) == 1
                    && libc::minor(meta.rdev()) == minor,
                "invalid sandbox device"
            );
            checked(
                unsafe { libc::mknod(cpath(&dst)?.as_ptr(), meta.mode(), meta.rdev()) } as i64,
            )?;
        }
    }
    Ok(())
}

pub struct GroupProcess {
    child: Child,
    pid: i32,
    armed: bool,
    status: Option<std::process::ExitStatus>,
    group_clean: bool,
    budget: Option<PathBuf>,
}
impl GroupProcess {
    pub fn pid(&self) -> i32 {
        self.pid
    }
    fn kill_group(&mut self) {
        if self.armed {
            unsafe {
                libc::kill(-self.pid, libc::SIGKILL);
            }
            self.armed = false;
        }
    }
    pub async fn stop(&mut self) -> anyhow::Result<std::process::ExitStatus> {
        if self.group_clean {
            return Ok(self.status.expect("clean group has reaped leader"));
        }
        // Signal before reaping the leader, so the process group cannot be reused.
        self.kill_group();
        let status = if let Some(status) = self.status {
            status
        } else {
            let status = tokio::time::timeout(Duration::from_secs(5), self.child.wait()).await??;
            self.status = Some(status);
            status
        };
        for _ in 0..200 {
            while unsafe { libc::waitpid(-self.pid, std::ptr::null_mut(), libc::WNOHANG) } > 0 {}
            if unsafe { libc::kill(-self.pid, 0) } < 0
                && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
            {
                self.group_clean = true;
                return Ok(status);
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        bail!("sandbox descendants did not exit")
    }
    pub async fn wait_report(
        &mut self,
        wall: Duration,
    ) -> anyhow::Result<(std::process::ExitStatus, String, String)> {
        let stdout = self.child.stdout.take();
        let stderr = self.child.stderr.take();
        let output = tokio::spawn(async move {
            let mut b = Vec::new();
            if let Some(s) = stdout {
                b = drain_capped(s).await?;
            }
            Ok::<_, std::io::Error>(b)
        });
        let errors = tokio::spawn(async move {
            let mut b = Vec::new();
            if let Some(s) = stderr {
                b = drain_capped(s).await?;
            }
            Ok::<_, std::io::Error>(b)
        });
        let deadline = tokio::time::Instant::now() + wall;
        loop {
            let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
            checked(unsafe {
                libc::waitid(
                    libc::P_PID,
                    self.pid as u32,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            } as i64)?;
            if unsafe { info.si_pid() } != 0 {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                self.kill_group();
                break;
            }
            if let Some(path) = &self.budget
                && !within_budget(path)?
            {
                self.stop().await?;
                bail!("scratch budget exceeded");
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        let status = self.stop().await?;
        let out = tokio::time::timeout(Duration::from_secs(2), output).await???;
        let err = tokio::time::timeout(Duration::from_secs(2), errors).await???;
        Ok((
            status,
            String::from_utf8_lossy(&out).into_owned(),
            String::from_utf8_lossy(&err).into_owned(),
        ))
    }
}
impl Drop for GroupProcess {
    fn drop(&mut self) {
        self.kill_group();
    }
}

fn within_budget(path: &Path) -> anyhow::Result<bool> {
    let mut pending = vec![path.to_path_buf()];
    let (mut count, mut bytes) = (0, 0_u64);
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let metadata = fs::symlink_metadata(entry.path())?;
            count += 1;
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                bytes = bytes.saturating_add(metadata.len());
            }
            if count > 10000 || bytes > 4_000_000_000 {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

async fn drain_capped(mut reader: impl tokio::io::AsyncRead + Unpin) -> std::io::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut result = Vec::new();
    let mut buffer = [0; 8192];
    loop {
        let n = reader.read(&mut buffer).await?;
        if n == 0 {
            return Ok(result);
        }
        result.extend_from_slice(&buffer[..n]);
        if result.len() > 65536 {
            result.drain(..result.len() - 65536);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[tokio::test]
    async fn cleanup_retry_does_not_treat_reaped_leader_as_clean_group() {
        let mut command = Command::new("/bin/sleep");
        command.arg("30").env_clear().kill_on_drop(true);
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().unwrap();
        let pid = child.id().unwrap() as i32;
        // Inject the state after a leader was reaped but group cleanup timed out.
        let mut process = GroupProcess {
            child,
            pid,
            armed: false,
            status: Some(std::process::ExitStatus::from_raw(0)),
            group_clean: false,
            budget: None,
        };
        let first = process.stop().await;
        let retry = process.stop().await;
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
        }
        tokio::time::timeout(Duration::from_secs(5), process.child.wait())
            .await
            .unwrap()
            .unwrap();
        assert!(first.is_err(), "live group reported clean");
        assert!(retry.is_err(), "cleanup timeout became success on retry");
    }

    #[tokio::test]
    async fn verbose_child_logs_are_drained_with_a_bounded_tail() {
        let mut input = vec![b'a'; 1_000_000];
        input.extend_from_slice(b"the final error");
        let tail = drain_capped(input.as_slice()).await.unwrap();
        assert_eq!(tail.len(), 65536);
        assert!(tail.ends_with(b"the final error"));
    }
}
