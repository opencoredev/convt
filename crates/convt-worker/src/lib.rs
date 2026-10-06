#[cfg(target_os = "linux")]
pub mod process_sandbox;
#[cfg(target_os = "linux")]
pub mod resource_domain;
use convt_server::{
    jobs::{self, Job},
    storage::Storage,
};
use sqlx::PgPool;
use std::{path::Path, time::Duration};
use tokio::process::Command;

pub enum Sandbox {
    Docker(String),
    #[cfg(target_os = "linux")]
    Process(process_sandbox::ProcessSandbox),
}
impl Sandbox {
    async fn stop(&self, name: &str) -> anyhow::Result<()> {
        match self {
            Self::Docker(_) => remove(name).await,
            #[cfg(target_os = "linux")]
            Self::Process(sandbox) => sandbox.stop(name).await,
        }
    }
}

async fn docker(args: &[&str]) -> anyhow::Result<std::process::Output> {
    let out = tokio::time::timeout(
        Duration::from_secs(30),
        Command::new("docker")
            .args(args)
            .kill_on_drop(true)
            .output(),
    )
    .await??;
    anyhow::ensure!(
        out.status.success(),
        "docker command failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(out)
}
pub async fn remove(name: &str) -> anyhow::Result<()> {
    for attempt in 0..3 {
        match docker(&["rm", "-f", name]).await {
            Ok(_) => return Ok(()),
            Err(error) if error.to_string().contains("No such container") => return Ok(()),
            Err(error) if attempt == 2 => return Err(error),
            Err(_) => tokio::time::sleep(Duration::from_millis(250)).await,
        }
    }
    unreachable!()
}
/// Register once and retain the receivers even while database calls await.
pub fn shutdown_listener() -> std::io::Result<tokio::sync::watch::Receiver<bool>> {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let mut interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
    let (sender, receiver) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        tokio::select! {
            _ = interrupt.recv() => {},
            _ = terminate.recv() => {},
        }
        let _ = sender.send(true);
    });
    Ok(receiver)
}
pub async fn wait_for_shutdown(receiver: &mut tokio::sync::watch::Receiver<bool>) {
    if !*receiver.borrow() {
        let _ = receiver.changed().await;
    }
}
/// Remove only checkout-labeled conversion containers whose attempt is no
/// longer live. Database failures prevent removal rather than guessing.
pub async fn reap(pool: &PgPool) -> anyhow::Result<()> {
    let label = format!(
        "label=convt.checkout={}",
        std::env::current_dir()?.display()
    );
    let output = docker(&[
        "ps",
        "-a",
        "--filter",
        "label=convt.role=conversion",
        "--filter",
        &label,
        "--format",
        "{{.Names}}",
    ])
    .await?;
    for name in String::from_utf8_lossy(&output.stdout).lines() {
        let Some((id, attempt)) = name
            .strip_prefix("convt-job_")
            .and_then(|value| value.rsplit_once('-'))
        else {
            continue;
        };
        let Ok(attempt) = attempt.parse::<i32>() else {
            continue;
        };
        if !jobs::attempt_is_live(pool, &format!("job_{id}"), attempt).await? {
            remove(name).await?;
        }
    }
    Ok(())
}

pub async fn convert(
    pool: &PgPool,
    storage: &dyn Storage,
    job: &Job,
    name: &str,
    image: &str,
) -> anyhow::Result<Vec<String>> {
    let work = tempfile::tempdir()?;
    let input = work.path().join(format!("input.{}", job.input_format));
    storage
        .download(
            job.input_key
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("missing input"))?,
            &input,
        )
        .await?;
    anyhow::ensure!(
        tokio::fs::metadata(&input).await?.len() == job.input_bytes.unwrap_or(0) as u64,
        "input size changed"
    );
    let runtime = std::env::var("CONVT_SANDBOX_RUNTIME").unwrap_or_else(|_| "runc".into());
    let mount = format!(
        "type=bind,src={},dst=/input.{},readonly",
        input.display(),
        job.input_format
    );
    let checkout = std::env::current_dir()?;
    let label = format!("convt.checkout={}", checkout.display());
    docker(&["create","--runtime",&runtime,"--name",name,"--label",&label,"--label","convt.role=conversion","--network","none","--read-only","--user","10001:10001","--cap-drop","ALL","--security-opt","no-new-privileges","--pids-limit","128","--memory","4g","--memory-swap","4g","--cpus","2","--ulimit","cpu=600:600","--ulimit","fsize=2000000000:2000000000","--tmpfs","/work:rw,nosuid,nodev,size=4g,uid=10001,gid=10001","--tmpfs","/tmp:rw,nosuid,nodev,size=256m,mode=1777","--log-driver","none","--env","HOME=/work","--env","XDG_CONFIG_HOME=/work/config","--env","XDG_CACHE_HOME=/work/cache","--env","CONVT_LICENSE_STORE=file","--mount",&mount,"--entrypoint","/usr/bin/timeout",image,"--signal=KILL","600","/bin/sh","-c",r#"/opt/convt/convt "$1" --to "$2" --out-dir /work/output --json; code=$?; printf '%s' "$code" > /work/exit; exec sleep 86400"#,"--",&format!("/input.{}",job.input_format),&job.target_format]).await?;
    docker(&["start", name]).await?;
    loop {
        let state = docker(&[
            "inspect",
            "--format",
            "{{.State.Running}} {{.State.ExitCode}}",
            name,
        ])
        .await?;
        let state = String::from_utf8_lossy(&state.stdout);
        anyhow::ensure!(state.starts_with("true"), "conversion container stopped");
        let exit = docker(&[
            "exec",
            name,
            "/bin/sh",
            "-c",
            "if test -f /work/exit; then cat /work/exit; fi",
        ])
        .await?;
        if !exit.stdout.is_empty() {
            anyhow::ensure!(exit.stdout == b"0", "conversion process failed");
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    let output = work.path().join("outputs");
    tokio::fs::create_dir(&output).await?;
    // Docker cp cannot read tmpfs on this host. Stream a bounded archive while
    // the supervisor is alive, opening regular outputs without following links.
    let archive = work.path().join("outputs.tar");
    let program = r#"import os,stat,tarfile,sys
root='/work/output'
names=sorted(os.listdir(root))
assert 1<=len(names)<=256
archive=tarfile.open(fileobj=sys.stdout.buffer,mode='w|')
total=0
for name in names:
 fd=os.open(root+'/'+name,os.O_RDONLY|os.O_NOFOLLOW)
 info=os.fstat(fd)
 assert stat.S_ISREG(info.st_mode) and 0<info.st_size<=2000000000
 total+=info.st_size
 assert total<=4000000000
 header=tarfile.TarInfo(name); header.size=info.st_size
 with os.fdopen(fd,'rb') as f: archive.addfile(header,f)
archive.close()
"#;
    let mut child = Command::new("docker")
        .args(["exec", name, "/usr/bin/python3", "-c", program])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let stdout = child.stdout.take().unwrap();
    let mut bounded = tokio::io::AsyncReadExt::take(stdout, 4_001_000_000);
    let mut destination = tokio::fs::File::create(&archive).await?;
    let written = tokio::io::copy(&mut bounded, &mut destination).await?;
    anyhow::ensure!(
        written < 4_001_000_000 && child.wait().await?.success(),
        "invalid output archive"
    );
    tokio::io::AsyncWriteExt::flush(&mut destination).await?;
    drop(destination);
    extract_outputs(&archive, &output)?;
    let mut entries = tokio::fs::read_dir(&output).await?;
    let mut paths = Vec::new();
    let mut total = 0;
    while let Some(entry) = entries.next_entry().await? {
        let metadata = entry.file_type().await?;
        anyhow::ensure!(
            metadata.is_file() && !metadata.is_symlink(),
            "non-regular output refused"
        );
        let bytes = entry.metadata().await?.len();
        anyhow::ensure!(bytes > 0 && bytes <= 2_000_000_000, "invalid output size");
        total += bytes;
        paths.push(entry.path());
        anyhow::ensure!(
            paths.len() <= 256 && total <= 4_000_000_000,
            "output budget exceeded"
        );
    }
    anyhow::ensure!(!paths.is_empty(), "conversion produced no files");
    paths.sort();
    anyhow::ensure!(
        jobs::renew(pool, job).await?,
        "lease lost before publication"
    );
    let mut keys = Vec::new();
    for path in paths {
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("invalid output name"))?;
        let key = format!("{}/attempt-{}/{filename}", job.id, job.attempt);
        storage.upload(&key, Path::new(&path)).await?;
        keys.push(key);
    }
    Ok(keys)
}
pub async fn run_job_backend(
    pool: &PgPool,
    storage: &dyn Storage,
    job: &Job,
    sandbox: &Sandbox,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<bool> {
    let name = format!("convt-{}-{}", job.id, job.attempt);
    let mut stopping = false;
    let result = {
        let conversion = async {
            match sandbox {
                Sandbox::Docker(image) => convert(pool, storage, job, &name, image).await,
                #[cfg(target_os = "linux")]
                Sandbox::Process(sandbox) => {
                    convert_process(pool, storage, job, &name, sandbox).await
                }
            }
        };
        tokio::pin!(conversion);
        let mut heartbeat = tokio::time::interval(Duration::from_secs(5));
        let timeout = tokio::time::sleep(Duration::from_secs(600));
        tokio::pin!(timeout);
        loop {
            tokio::select! {
                result=&mut conversion=>break result,
                _=heartbeat.tick()=>{match jobs::renew(pool,job).await { Ok(true) => {}, Ok(false) => break Err(anyhow::anyhow!("cancelled or lease lost")), Err(error) => break Err(error.into()) }},
                _=&mut timeout=>break Err(anyhow::anyhow!("wall-clock limit")),
                _=wait_for_shutdown(shutdown)=>{stopping=true;break Err(anyhow::anyhow!("worker shutdown"));}
            }
        }
    };
    sandbox.stop(&name).await?;
    match result {
        Ok(keys) => {
            if !jobs::finish(pool, job, &keys, None).await? {
                for key in keys {
                    storage.delete(&key).await?;
                }
            }
        }
        Err(e) => {
            tracing::warn!(job_id=job.id,error=%e,"conversion failed");
            jobs::finish(
                pool,
                job,
                &[],
                Some(if stopping {
                    "worker_shutdown"
                } else {
                    "conversion_failed"
                }),
            )
            .await?;
            storage
                .delete_prefix(&format!("{}/attempt-{}/", job.id, job.attempt))
                .await?;
        }
    }
    Ok(stopping)
}

fn extract_outputs(archive: &Path, output: &Path) -> anyhow::Result<()> {
    let mut archive = tar::Archive::new(std::fs::File::open(archive)?);
    let mut count = 0;
    let mut total = 0;
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        anyhow::ensure!(
            entry.header().entry_type().is_file()
                && path.components().count() == 1
                && matches!(
                    path.components().next(),
                    Some(std::path::Component::Normal(_))
                ),
            "unsafe output archive path"
        );
        count += 1;
        total += entry.size();
        anyhow::ensure!(
            count <= 256 && total <= 4_000_000_000,
            "output budget exceeded"
        );
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(output.join(path))?;
        std::io::copy(&mut entry, &mut file)?;
    }
    Ok(())
}

pub async fn run_job(
    pool: &PgPool,
    storage: &dyn Storage,
    job: &Job,
    image: &str,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<bool> {
    run_job_backend(pool, storage, job, &Sandbox::Docker(image.into()), shutdown).await
}

#[cfg(target_os = "linux")]
async fn convert_process(
    pool: &PgPool,
    storage: &dyn Storage,
    job: &Job,
    name: &str,
    sandbox: &process_sandbox::ProcessSandbox,
) -> anyhow::Result<Vec<String>> {
    let work = tempfile::tempdir()?;
    let input = work.path().join(format!("input.{}", job.input_format));
    storage
        .download(
            job.input_key
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("missing input"))?,
            &input,
        )
        .await?;
    anyhow::ensure!(
        tokio::fs::metadata(&input).await?.len() == job.input_bytes.unwrap_or(0) as u64,
        "input size changed"
    );
    let root = sandbox.prepare(&input, &job.input_format)?;
    let active = sandbox.start(name, root, &job.target_format).await?;
    let mut active = active.lock().await;
    let (status, _, error) = active.process.wait_report(Duration::from_secs(600)).await?;
    anyhow::ensure!(status.success(), "conversion process failed: {error}");
    let output = active.root.root.path().join("output/files");
    let mut paths = Vec::new();
    let mut total = 0_u64;
    for entry in std::fs::read_dir(&output)? {
        let entry = entry?;
        let meta = std::fs::symlink_metadata(entry.path())?;
        anyhow::ensure!(
            meta.is_file()
                && !meta.file_type().is_symlink()
                && meta.len() > 0
                && meta.len() <= 2_000_000_000,
            "invalid output"
        );
        total = total
            .checked_add(meta.len())
            .ok_or_else(|| anyhow::anyhow!("output budget overflow"))?;
        paths.push(entry.path());
        anyhow::ensure!(
            paths.len() <= 256 && total <= 4_000_000_000,
            "output budget exceeded"
        );
    }
    anyhow::ensure!(!paths.is_empty(), "conversion produced no files");
    paths.sort();
    anyhow::ensure!(
        jobs::renew(pool, job).await?,
        "lease lost before publication"
    );
    let mut keys = Vec::new();
    for path in paths {
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("invalid output name"))?;
        let key = format!("{}/attempt-{}/{filename}", job.id, job.attempt);
        storage.upload(&key, &path).await?;
        keys.push(key);
    }
    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn shutdown_stays_latched_across_unrelated_waits() {
        let (sender, mut receiver) = tokio::sync::watch::channel(false);
        sender.send(true).unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        tokio::time::timeout(Duration::from_millis(20), wait_for_shutdown(&mut receiver))
            .await
            .unwrap();
        tokio::time::timeout(Duration::from_millis(20), wait_for_shutdown(&mut receiver))
            .await
            .unwrap();
    }
    #[test]
    fn output_collection_refuses_links_and_traversal() {
        for (name, kind) in [
            ("../escape", tar::EntryType::Regular),
            ("link", tar::EntryType::Symlink),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("output.tar");
            let mut builder = tar::Builder::new(std::fs::File::create(&path).unwrap());
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(kind);
            header.set_size(0);
            header.set_mode(0o644);
            header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
            header.set_cksum();
            builder.append(&header, std::io::empty()).unwrap();
            builder.finish().unwrap();
            let output = temp.path().join("files");
            std::fs::create_dir(&output).unwrap();
            assert!(extract_outputs(&path, &output).is_err());
            assert!(!temp.path().join("escape").exists());
        }
    }
}
