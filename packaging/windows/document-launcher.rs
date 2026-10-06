//! Pack entry point: private DLL search path and cancellation of the Office tree.
#![windows_subsystem = "console"]
use std::os::windows::process::CommandExt;
use std::{
    env,
    ffi::c_void,
    process::{Command, exit},
};

type Handle = *mut c_void;
#[repr(C)]
#[derive(Default)]
struct BasicLimits {
    process_time: i64,
    job_time: i64,
    flags: u32,
    min_working_set: usize,
    max_working_set: usize,
    active_processes: u32,
    affinity: usize,
    priority: u32,
    scheduling: u32,
}
#[repr(C)]
#[derive(Default)]
struct ExtendedLimits {
    basic: BasicLimits,
    io_counters: [u64; 6],
    process_memory: usize,
    job_memory: usize,
    peak_process_memory: usize,
    peak_job_memory: usize,
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> Handle;
    fn SetInformationJobObject(job: Handle, class: i32, info: *const c_void, size: u32) -> i32;
    fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
    fn GetCurrentProcess() -> Handle;
    fn CloseHandle(handle: Handle) -> i32;
}

fn contain_children() -> std::io::Result<Handle> {
    // SAFETY: unnamed job, correctly laid out JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    // live handles. Children inherit job membership before they execute.
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let mut limits = ExtendedLimits::default();
        limits.basic.flags = 0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        if SetInformationJobObject(
            job,
            9,
            (&limits as *const ExtendedLimits).cast(),
            std::mem::size_of::<ExtendedLimits>() as u32,
        ) == 0
            || AssignProcessToJobObject(job, GetCurrentProcess()) == 0
        {
            let error = std::io::Error::last_os_error();
            CloseHandle(job);
            return Err(error);
        }
        // Keep this handle until process exit. Explicitly closing it would kill
        // the launcher too; OS closure on cancellation kills every descendant.
        Ok(job)
    }
}

fn main() {
    let result = (|| -> std::io::Result<_> {
        let _job = contain_children()?;
        let exe = env::current_exe()?;
        let program = exe.parent().unwrap().join("libreoffice/program");
        let system = env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        let path = env::join_paths([
            program.clone(),
            std::path::PathBuf::from(system).join("System32"),
        ])
        .map_err(std::io::Error::other)?;
        Command::new(program.join("soffice.com"))
            .args(env::args_os().skip(1))
            .env("PATH", path)
            .creation_flags(0x08000000)
            .status()
    })();
    match result {
        Ok(status) => exit(status.code().unwrap_or(1)),
        Err(error) => {
            eprintln!("document pack: {error}");
            exit(1);
        }
    }
}
