fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(target_os = "linux")]
    if args.first().map(String::as_str) == Some("--sandbox-child") {
        return convt_worker::process_sandbox::child_main(&args[1..]);
    }
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run(args))
}

// Off Linux the sandbox commands that read `args` and the process sandbox variant are
// compiled out, which leaves `args` unused and Docker the only `Sandbox`.
#[cfg_attr(
    not(target_os = "linux"),
    allow(unused_variables, irrefutable_let_patterns)
)]
async fn run(args: Vec<String>) -> anyhow::Result<()> {
    convt_server::install_crypto();
    #[cfg(target_os = "linux")]
    if let Some(action) = args.first().map(String::as_str)
        && action.starts_with("--sandbox-")
    {
        let sandbox = convt_worker::process_sandbox::ProcessSandbox::from_env()?;
        match action {
            "--sandbox-gate" => println!("{}", sandbox.gate().await?),
            "--sandbox-test" => {
                println!("{}", sandbox.gate().await?);
                sandbox.verify_resources().await?;
            }
            "--sandbox-office" => sandbox.office_probe().await?,
            "--sandbox-formats" => println!("{}", sandbox.formats().await?),
            "--sandbox-convert" => {
                anyhow::ensure!(
                    args.len() == 4,
                    "usage: --sandbox-convert INPUT TARGET OUTPUT_DIR"
                );
                let input = std::path::Path::new(&args[1]);
                let extension = input
                    .extension()
                    .and_then(|x| x.to_str())
                    .ok_or_else(|| anyhow::anyhow!("missing extension"))?;
                let root = sandbox.prepare(input, extension)?;
                let mut process = sandbox.spawn(&root, &args[2], Default::default(), true)?;
                let (status, _, error) = process
                    .wait_report(std::time::Duration::from_secs(600))
                    .await?;
                anyhow::ensure!(status.success(), "conversion failed: {error}");
                std::fs::create_dir_all(&args[3])?;
                for entry in std::fs::read_dir(root.root.path().join("output/files"))? {
                    let entry = entry?;
                    anyhow::ensure!(entry.file_type()?.is_file(), "unsafe output");
                    std::fs::copy(
                        entry.path(),
                        std::path::Path::new(&args[3]).join(entry.file_name()),
                    )?;
                }
                println!("PASS sandbox conversion");
            }
            "--sandbox-orphan" => {
                let input = tempfile::NamedTempFile::new_in(&sandbox.jobs)?;
                let root = sandbox.prepare(input.path(), "txt")?;
                let child = sandbox.spawn(&root, "probe:tree", Default::default(), true)?;
                println!("{}", child.pid());
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
            }
            _ => anyhow::bail!("unknown sandbox command"),
        }
        return Ok(());
    }
    let mut shutdown = convt_worker::shutdown_listener()?;
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let backend = std::env::var("CONVT_SANDBOX_BACKEND").unwrap_or_else(|_| "process".into());
    let sandbox = match backend.as_str() {
        #[cfg(target_os = "linux")]
        "process" => {
            let sandbox = convt_worker::process_sandbox::ProcessSandbox::from_env()?;
            let gate = sandbox.gate().await?;
            tracing::info!(report=%gate,"sandbox gate");
            let actual = sandbox.formats().await?;
            let manifest: serde_json::Value =
                serde_json::from_str(include_str!("../../convt-server/cloud-formats.json"))?;
            for advertised in manifest["formats"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("invalid capabilities"))?
            {
                let supported = actual
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|f| f["id"] == advertised["id"])
                    .ok_or_else(|| anyhow::anyhow!("missing sandbox format"))?;
                for target in advertised["targets"].as_array().unwrap() {
                    anyhow::ensure!(
                        supported["targets"].as_array().unwrap().contains(target),
                        "sandbox cannot provide advertised conversion {} -> {}",
                        advertised["id"],
                        target
                    );
                }
            }
            convt_worker::Sandbox::Process(sandbox)
        }
        #[cfg(not(target_os = "linux"))]
        "process" => anyhow::bail!("process sandbox requires Linux"),
        "docker" => convt_worker::Sandbox::Docker(
            std::env::var("CONVT_SANDBOX_IMAGE").unwrap_or_else(|_| "convt-sandbox:local".into()),
        ),
        _ => anyhow::bail!("CONVT_SANDBOX_BACKEND must be process or docker"),
    };
    let pool = convt_server::db::connect(&std::env::var("DATABASE_URL")?).await?;
    convt_server::migrations::check(&pool).await?;
    let storage = convt_server::storage::S3Storage::from_env()?;
    if let convt_worker::Sandbox::Docker(image) = &sandbox {
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../../convt-server/cloud-formats.json"))?;
        let actual = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            tokio::process::Command::new("docker")
                .args(["image", "inspect", "--format", "{{.Id}}", image])
                .kill_on_drop(true)
                .output(),
        )
        .await??;
        anyhow::ensure!(
            actual.status.success()
                && String::from_utf8_lossy(&actual.stdout).trim()
                    == manifest["image_id"].as_str().unwrap_or(""),
            "Sandbox image differs from generated capabilities. Regenerate capabilities and rebuild server and worker."
        );
        convt_worker::reap(&pool).await?;
        let reaper_pool = pool.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
            loop {
                interval.tick().await;
                if let Err(error) = convt_worker::reap(&reaper_pool).await {
                    tracing::error!(%error, "orphan container cleanup failed");
                }
            }
        });
    }
    let worker = convt_server::ids::new_id("worker");
    let meter =
        std::env::var("POLAR_ACCESS_TOKEN")
            .ok()
            .map(|token| convt_server::meter::PolarMeter {
                token,
                endpoint: std::env::var("POLAR_API_URL")
                    .unwrap_or_else(|_| "https://api.polar.sh".into()),
            });
    loop {
        if *shutdown.borrow() {
            break;
        }
        if let Some(meter) = &meter
            && let Err(e) = convt_server::meter::drain(&pool, meter).await
        {
            tracing::warn!(error=%e,"meter sender will retry");
        }
        if *shutdown.borrow() {
            break;
        }
        if let Some(job) = convt_server::jobs::claim(&pool, &worker).await? {
            if convt_worker::run_job_backend(&pool, &storage, &job, &sandbox, &mut shutdown).await?
            {
                break;
            }
        } else {
            tokio::select! {_=convt_worker::wait_for_shutdown(&mut shutdown)=>break,_=tokio::time::sleep(std::time::Duration::from_secs(1))=>{}}
        }
    }
    Ok(())
}
