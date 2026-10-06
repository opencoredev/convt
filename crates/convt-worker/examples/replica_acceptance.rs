//! Real HTTP + three-process acceptance; requires Linux root, python3, a gated
//! /srv/convt-template, MinIO S3_*, DATABASE_URL (server role) and the test-db.sh admin URL.
//! Creates its own disposable database from convt_template. No Docker.
//! Run: CONVT_SANDBOX_VERIFIED=1 CONVT_ACCEPTANCE_FIXTURES=/fixtures \
//!   /opt/convt/replica_acceptance
use anyhow::{Context, Result, ensure};
use convt_server::storage::{S3Storage, Storage};
use serde_json::{Value, json};
use sqlx::{ConnectOptions, PgPool, Row, postgres::PgConnectOptions};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    net::TcpListener,
    path::{Path, PathBuf},
    process::Stdio,
    str::FromStr,
    time::{Duration, Instant},
};
use tokio::{
    io::AsyncWriteExt,
    process::{Child, Command},
};

struct Owned {
    children: Vec<Child>,
    prefixes: Vec<String>,
    evidence: PathBuf,
}
impl Owned {
    fn spawn(&mut self, binary: &Path, name: &str, env: &[(&str, String)]) -> Result<()> {
        let log = File::create(self.evidence.join(format!("{name}.log")))?;
        let mut command = Command::new(binary);
        command
            .envs(env.iter().map(|(k, v)| (*k, v)))
            // Acceptance must never deliver usage to a real billing provider.
            .env_remove("POLAR_ACCESS_TOKEN")
            .stdout(Stdio::from(log.try_clone()?))
            .stderr(Stdio::from(log))
            .kill_on_drop(true);
        self.children
            .push(command.spawn().with_context(|| format!("launch {name}"))?);
        Ok(())
    }
    fn alive(&mut self) -> Result<()> {
        for (index, child) in self.children.iter_mut().enumerate() {
            ensure!(
                child.try_wait()?.is_none(),
                "owned process {index} exited; inspect evidence logs"
            );
        }
        Ok(())
    }
    async fn stop(&mut self) -> Result<()> {
        for child in &mut self.children {
            if child.try_wait()?.is_none()
                && let Some(pid) = child.id()
            {
                // Only IDs returned by our spawn; no name-based process killing.
                #[cfg(unix)]
                unsafe {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
                #[cfg(not(unix))]
                let _ = pid;
            }
        }
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let mut live = false;
            for child in &mut self.children {
                live |= child.try_wait()?.is_none();
            }
            if !live {
                break;
            }
            if Instant::now() >= deadline {
                for child in &mut self.children {
                    if child.try_wait()?.is_none() {
                        child.start_kill()?;
                    }
                }
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        for child in &mut self.children {
            tokio::time::timeout(Duration::from_secs(5), child.wait()).await??;
        }
        Ok(())
    }
}

// Python's standard HTTP client is already present in the production image.
// Signed URLs and auth go over stdin, never into argv, logs or exception text.
async fn http(method: &str, url: &str, key: Option<&str>, body: Option<&Path>) -> Result<Vec<u8>> {
    const CLIENT: &str = r#"
import json, os, sys, urllib.request
config = json.load(sys.stdin)
headers = {}
if config['key'] is not None:
    headers['Authorization'] = 'Bearer ' + config['key']
body = None
if config['body'] is not None:
    body = open(config['body'], 'rb')
    headers['Content-Length'] = str(os.fstat(body.fileno()).st_size)
    headers['Content-Type'] = ('application/octet-stream' if config['method'] == 'PUT' else 'application/json')
request = urllib.request.Request(config['url'], data=body, headers=headers, method=config['method'])
try:
    with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request, timeout=30) as response:
        while chunk := response.read(65536):
            sys.stdout.buffer.write(chunk)
except Exception:
    sys.exit(1)
finally:
    if body is not None:
        body.close()
"#;
    let config =
        serde_json::to_vec(&json!({ "method": method, "url": url, "key": key, "body": body }))?;
    let mut child = Command::new("python3")
        .args(["-c", CLIENT])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("python3 is required")?;
    let mut stdin = child.stdin.take().context("HTTP helper stdin")?;
    stdin.write_all(&config).await?;
    drop(stdin);
    let output = tokio::time::timeout(Duration::from_secs(35), child.wait_with_output()).await??;
    ensure!(
        output.status.success(),
        "HTTP {method} failed (URL and credentials withheld)"
    );
    Ok(output.stdout)
}

async fn api(
    method: &str,
    base: &str,
    route: &str,
    key: &str,
    body: Option<&Path>,
) -> Result<Value> {
    Ok(serde_json::from_slice(
        &http(method, &format!("{base}{route}"), Some(key), body).await?,
    )?)
}

struct Fixture {
    input: &'static str,
    target: &'static str,
    path: PathBuf,
}
struct Job {
    id: String,
    input: &'static str,
    target: &'static str,
}
async fn upload(base: &str, key: &str, fixture: &Fixture, owned: &mut Owned) -> Result<Job> {
    let request = owned.evidence.join("reserve.json");
    fs::write(
        &request,
        serde_json::to_vec(&json!({
            "input_format": fixture.input, "target_format": fixture.target,
            "input_bytes": fs::metadata(&fixture.path)?.len()
        }))?,
    )?;
    let response = api("POST", base, "/v1/jobs", key, Some(&request)).await?;
    let id = response["job"]["id"]
        .as_str()
        .context("reservation job id")?
        .to_owned();
    // Register cleanup before attempting the upload or any later operation.
    owned.prefixes.push(format!("{id}/"));
    let url = response["upload_url"]
        .as_str()
        .context("reservation upload URL")?;
    http("PUT", url, None, Some(&fixture.path)).await?;
    Ok(Job {
        id,
        input: fixture.input,
        target: fixture.target,
    })
}
fn signature(bytes: &[u8], target: &str) -> bool {
    match target {
        "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpeg" => bytes.starts_with(b"\xff\xd8\xff") && bytes.ends_with(b"\xff\xd9"),
        "webm" => bytes.starts_with(b"\x1a\x45\xdf\xa3"),
        "pdf" => bytes.starts_with(b"%PDF-") && bytes.windows(5).any(|b| b == b"%%EOF"),
        _ => false,
    }
}
async fn seed(pool: &PgPool, user: &str, key: &str) -> Result<()> {
    let email = format!("{user}@convt.test");
    sqlx::query(
        "insert into users (id,name,email,email_verified) values ($1,'replica acceptance',$2,true)",
    )
    .bind(user)
    .bind(&email)
    .execute(pool)
    .await?;
    sqlx::query("insert into subscriptions (id,user_id,email,kind,status,provider_subscription_id,current_period_start,current_period_end,spend_cap_cents,card_seen_at) values ($1,$2,$3,'api','active',$1,date_trunc('month',now()),now()+interval '1 year',100,now())")
        .bind(format!("sub_{user}")).bind(user).bind(&email).execute(pool).await?;
    sqlx::query("insert into api_keys (id,user_id,name,prefix,secret_hash) values ($1,$2,'replica acceptance',$3,$4)")
        .bind(format!("key_{user}")).bind(user).bind(&key[..17])
        .bind(&convt_server::api_keys::hash_key(key)[..]).execute(pool).await?;
    Ok(())
}

async fn run(pool: &PgPool, server_options: &PgConnectOptions, owned: &mut Owned) -> Result<()> {
    let sibling = std::env::current_exe()?
        .parent()
        .context("example directory")?
        .to_path_buf();
    let worker = std::env::var_os("CONVT_ACCEPTANCE_WORKER")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if sibling.join("convt-worker").is_file() {
                sibling.join("convt-worker")
            } else {
                PathBuf::from("target/debug/convt-worker")
            }
        });
    let server = std::env::var_os("CONVT_ACCEPTANCE_SERVER")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if sibling.join("convt-server").is_file() {
                sibling.join("convt-server")
            } else {
                PathBuf::from("target/debug/convt-server")
            }
        });
    let fixture_dir = PathBuf::from(
        std::env::var("CONVT_ACCEPTANCE_FIXTURES").unwrap_or_else(|_| "/fixtures".into()),
    );
    let svg = owned.evidence.join("input.svg");
    fs::write(&svg, br##"<svg xmlns="http://www.w3.org/2000/svg" width="96" height="64"><rect width="96" height="64" fill="#196ac5"/></svg>"##)?;
    let fixtures = [
        Fixture {
            input: "mp4",
            target: "webm",
            path: fixture_dir.join("video.mp4"),
        },
        Fixture {
            input: "txt",
            target: "pdf",
            path: fixture_dir.join("document.txt"),
        },
        Fixture {
            input: "pdf",
            target: "png",
            path: fixture_dir.join("document.pdf"),
        },
        Fixture {
            input: "svg",
            target: "png",
            path: svg,
        },
        Fixture {
            input: "png",
            target: "jpeg",
            path: fixture_dir.join("image.png"),
        },
    ];
    for fixture in &fixtures {
        ensure!(fixture.path.is_file(), "missing {} fixture", fixture.input);
    }
    let user = convt_server::ids::new_id("usr_replica");
    let suffix = convt_server::ids::new_id("key").replace('_', "");
    let key = format!("cvt_live_{:0<32}", &suffix[..suffix.len().min(32)]);
    seed(pool, &user, &key).await?;
    let socket = TcpListener::bind("127.0.0.1:0")?;
    let port = socket.local_addr()?.port();
    drop(socket);
    let base = format!("http://127.0.0.1:{port}"); // Internal test client, never a shared preview link.
    owned.spawn(
        &server,
        "server",
        &[
            ("PORT", port.to_string()),
            ("CONVT_BIND_HOST", "127.0.0.1".into()),
            ("DATABASE_URL", server_options.to_url_lossy().to_string()),
            (
                "CONVT_WEB_TOKEN_SECRET",
                format!("acceptance-only-{user}-{user}"),
            ),
        ],
    )?;
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        owned.alive()?;
        if http("GET", &format!("{base}/health"), None, None)
            .await
            .is_ok_and(|body| body == b"ok")
        {
            break;
        }
        ensure!(Instant::now() < deadline, "server readiness timed out");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    println!("PASS actual server health, port {port}");
    let mut names = Vec::new();
    for index in 0..3 {
        let name = format!("replica-{index}-{user}");
        // sqlx's lossy serializer omits application_name. Preserve it explicitly.
        let mut database_url = server_options.to_url_lossy();
        database_url
            .query_pairs_mut()
            .append_pair("application_name", &name);
        let database_url = database_url.to_string();
        owned.spawn(
            &worker,
            &format!("worker-{index}"),
            &[
                ("DATABASE_URL", database_url),
                ("CONVT_SANDBOX_BACKEND", "process".into()),
                (
                    "CONVT_SANDBOX_JOBS",
                    owned
                        .evidence
                        .join(format!("jobs-{index}"))
                        .display()
                        .to_string(),
                ),
                (
                    "CONVT_SANDBOX_TEMPLATE",
                    std::env::var("CONVT_SANDBOX_TEMPLATE")
                        .unwrap_or_else(|_| "/srv/convt-template".into()),
                ),
            ],
        )?;
        names.push(name);
    }
    // A DB connection is opened only AFTER each binary's sandbox gate and
    // advertised-format validation. Wait for all three before queuing anything.
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        owned.alive()?;
        let ready: i64 = sqlx::query_scalar("select count(distinct application_name) from pg_stat_activity where datname=current_database() and application_name=any($1)")
            .bind(&names).fetch_one(pool).await?;
        if ready == 3 {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "three gated workers did not become ready"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    println!("PASS three actual worker binaries gated and connected, separate jobs directories");
    let mut jobs = Vec::new();
    for index in 0..12 {
        jobs.push(upload(&base, &key, &fixtures[index % fixtures.len()], owned).await?);
    }
    let mut starts = tokio::task::JoinSet::new();
    for job in &jobs {
        let (base, key, id) = (base.clone(), key.clone(), job.id.clone());
        starts.spawn(async move {
            api("POST", &base, &format!("/v1/jobs/{id}/start"), &key, None).await
        });
    }
    while let Some(result) = starts.join_next().await {
        result??;
    }
    println!("PASS 12 real HTTP reservations, exact-length S3 uploads and sealed starts");
    let deadline = Instant::now() + Duration::from_secs(600);
    loop {
        owned.alive()?;
        let rows = sqlx::query("select id,status,error_code from cloud_jobs where user_id=$1")
            .bind(&user)
            .fetch_all(pool)
            .await?;
        let mut complete = 0;
        for row in rows {
            let status: String = row.try_get("status")?;
            ensure!(
                !matches!(status.as_str(), "failed" | "cancelled"),
                "job {} ended {status}: {:?}",
                row.try_get::<String, _>("id")?,
                row.try_get::<Option<String>, _>("error_code")?
            );
            complete += usize::from(status == "succeeded");
        }
        if complete == 12 {
            break;
        }
        ensure!(Instant::now() < deadline, "conversion deadline exceeded");
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let mut claims = BTreeMap::<String, usize>::new();
    for job in &jobs {
        let row = sqlx::query("select attempt,lease_owner,reservation from cloud_jobs where id=$1")
            .bind(&job.id)
            .fetch_one(pool)
            .await?;
        ensure!(
            row.try_get::<i32, _>("attempt")? == 1,
            "job {} retried",
            job.id
        );
        ensure!(
            row.try_get::<String, _>("reservation")? == "settled",
            "job reservation not settled"
        );
        let worker: String = row.try_get("lease_owner")?;
        *claims.entry(worker).or_default() += 1;
        let view = api("GET", &base, &format!("/v1/jobs/{}", job.id), &key, None).await?;
        ensure!(
            view["status"] == "succeeded" && view["attempt"] == 1,
            "HTTP job status differs"
        );
        let download = api(
            "GET",
            &base,
            &format!("/v1/jobs/{}/download", job.id),
            &key,
            None,
        )
        .await?;
        let outputs = download["outputs"].as_array().context("download outputs")?;
        ensure!(!outputs.is_empty(), "job {} has no outputs", job.id);
        for (index, output) in outputs.iter().enumerate() {
            let bytes = http(
                "GET",
                output["url"].as_str().context("download URL")?,
                None,
                None,
            )
            .await?;
            ensure!(
                signature(&bytes, job.target),
                "bad {} output signature for {}",
                job.target,
                job.id
            );
            fs::write(
                owned
                    .evidence
                    .join(format!("{}-{index}.{}", job.id, job.target)),
                &bytes,
            )?;
        }
        let usage = sqlx::query("select count(*)::bigint n,coalesce(sum(quantity),0)::bigint quantity,coalesce(sum(amount_cents),0)::bigint cents from usage_events where job_id=$1 and user_id=$2 and kind='api_conversion' and api_key_id=$3 and subscription_id=$4")
            .bind(&job.id).bind(&user).bind(format!("key_{user}")).bind(format!("sub_{user}")).fetch_one(pool).await?;
        ensure!(
            usage.try_get::<i64, _>("n")? == 1
                && usage.try_get::<i64, _>("quantity")? == 1
                && usage.try_get::<i64, _>("cents")? == 1,
            "usage is not exactly once for {}",
            job.id
        );
        println!(
            "PASS {} {} -> {} signatures, attempt=1, usage=1",
            job.id, job.input, job.target
        );
    }
    ensure!(
        claims.len() == 3,
        "only {} of 3 workers claimed jobs: {claims:?}",
        claims.len()
    );
    println!("PASS all three DB lease_owner IDs claimed jobs: {claims:?}");
    let facts: i64 = sqlx::query_scalar("select count(*) from usage_events where user_id=$1")
        .bind(&user)
        .fetch_one(pool)
        .await?;
    ensure!(facts == 12, "unexpected additional usage facts");
    println!("PASS exactly 12 append-only usage facts; financial rows retained in disposable DB");
    if let Ok(path) = std::env::var("CONVT_ACCEPTANCE_CANCEL_FIXTURE") {
        cancellation(pool, &base, &key, PathBuf::from(path), owned).await?;
    } else {
        println!(
            "NOT CHECKED real API cancellation process tree: set CONVT_ACCEPTANCE_CANCEL_FIXTURE to a sufficiently long MP4"
        );
    }
    if let Ok(directory) = std::env::var("CONVT_ACCEPTANCE_SDK_EXCHANGE") {
        use std::os::unix::fs::PermissionsExt;
        let directory = PathBuf::from(directory);
        fs::create_dir_all(&directory)?;
        let file = directory.join("sdk.json");
        let mut options = fs::OpenOptions::new();
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = options
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(file)?;
        std::io::Write::write_all(
            &mut file,
            &serde_json::to_vec(&json!({"baseUrl":base,"apiKey":key}))?,
        )?;
        fs::set_permissions(
            directory.join("sdk.json"),
            fs::Permissions::from_mode(0o600),
        )?;
        println!("SDK exchange ready");
        let deadline = Instant::now() + Duration::from_secs(120);
        while !directory.join("sdk.done").exists() {
            owned.alive()?;
            ensure!(Instant::now() < deadline, "SDK acceptance timeout");
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        let ids: Vec<String> = serde_json::from_slice(&fs::read(directory.join("sdk.done"))?)?;
        ensure!(ids.len() == 4, "SDK must convert four real types");
        for id in &ids {
            ensure!(
                id.starts_with("job_")
                    && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
                "invalid SDK job id"
            );
            owned.prefixes.push(format!("{id}/"));
        }
        let count:i64=sqlx::query_scalar("select count(*) from usage_events where job_id=any($1) and user_id=$2 and kind='api_conversion'").bind(&ids).bind(&user).fetch_one(pool).await?;
        ensure!(count == 4, "SDK usage must settle exactly once");
        fs::remove_file(directory.join("sdk.json"))?;
        println!("PASS four external SDK conversions settled exactly once");
    }
    let queued: i64 = sqlx::query_scalar("select count(*) from cloud_jobs where status in ('created','queued','running') or reservation='open'")
        .fetch_one(pool).await?;
    ensure!(queued == 0, "disposable queue has {queued} unfinished jobs");
    fs::write(
        owned.evidence.join("summary.json"),
        serde_json::to_vec_pretty(&json!({
            "jobs": jobs.iter().map(|job| json!({"id": job.id, "input": job.input, "target": job.target, "attempt": 1, "usage_count": 1})).collect::<Vec<_>>(),
            "worker_claims": claims, "usage_facts": facts, "unfinished_queue": queued
        }))?,
    )?;
    println!("PASS no queue left: 0 created/queued/running/open reservations");
    Ok(())
}

// Linux /proc identity includes start ticks to avoid confusing a reused PID.
fn identity(pid: u32) -> Option<(u32, u64)> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let fields: Vec<_> = stat.rsplit_once(')')?.1.split_whitespace().collect();
    Some((fields.get(1)?.parse().ok()?, fields.get(19)?.parse().ok()?))
}
fn descendants(parents: &[u32]) -> BTreeMap<u32, u64> {
    let mut processes = BTreeMap::new();
    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            if let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>()
                && let Some((parent, ticks)) = identity(pid)
            {
                processes.insert(pid, (parent, ticks));
            }
        }
    }
    let mut found = BTreeMap::new();
    loop {
        let old = found.len();
        for (&pid, &(parent, ticks)) in &processes {
            if parents.contains(&parent) || found.contains_key(&parent) {
                found.insert(pid, ticks);
            }
        }
        if old == found.len() {
            return found;
        }
    }
}
async fn cancellation(
    pool: &PgPool,
    base: &str,
    key: &str,
    path: PathBuf,
    owned: &mut Owned,
) -> Result<()> {
    let job = upload(
        base,
        key,
        &Fixture {
            input: "mp4",
            target: "webm",
            path,
        },
        owned,
    )
    .await?;
    api(
        "POST",
        base,
        &format!("/v1/jobs/{}/start", job.id),
        key,
        None,
    )
    .await?;
    let parents: Vec<_> = owned
        .children
        .iter()
        .skip(1)
        .filter_map(Child::id)
        .collect();
    let deadline = Instant::now() + Duration::from_secs(30);
    let mut seen;
    loop {
        owned.alive()?;
        let status: String = sqlx::query_scalar("select status from cloud_jobs where id=$1")
            .bind(&job.id)
            .fetch_one(pool)
            .await?;
        ensure!(
            status != "succeeded" && status != "failed",
            "cancellation fixture completed before observation; supply a longer MP4"
        );
        seen = descendants(&parents);
        if status == "running" && !seen.is_empty() {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "no running cancellation process tree observed"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // Capture converter grandchildren as well as the sandbox supervisor.
    tokio::time::sleep(Duration::from_millis(100)).await;
    seen.extend(descendants(&parents));
    ensure!(seen.len() >= 2, "converter child process was not observed");
    let response = api(
        "POST",
        base,
        &format!("/v1/jobs/{}/cancel", job.id),
        key,
        None,
    )
    .await?;
    ensure!(
        response["status"] == "cancelled",
        "cancel endpoint did not cancel running job"
    );
    let deadline = Instant::now() + Duration::from_secs(25);
    loop {
        owned.alive()?;
        if seen
            .iter()
            .all(|(&pid, &ticks)| identity(pid).is_none_or(|(_, now)| now != ticks))
            && descendants(&parents).is_empty()
        {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "cancelled sandbox process tree survived"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let usage: i64 = sqlx::query_scalar("select count(*) from usage_events where job_id=$1")
        .bind(&job.id)
        .fetch_one(pool)
        .await?;
    ensure!(usage == 0, "cancelled job billed");
    println!(
        "PASS real API running-job cancellation; {} observed process identities gone; no usage",
        seen.len()
    );
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    ensure!(cfg!(target_os = "linux"), "acceptance requires Linux");
    #[cfg(unix)]
    ensure!(
        unsafe { libc::geteuid() } == 0,
        "acceptance must run as root inside the supplied container"
    );
    ensure!(
        std::env::var("CONVT_SANDBOX_VERIFIED").as_deref() == Ok("1"),
        "run the real sandbox gate before setting CONVT_SANDBOX_VERIFIED=1"
    );
    ensure!(
        !Path::new("/var/run/docker.sock").exists(),
        "acceptance requires a container without a Docker socket"
    );
    let server_options = PgConnectOptions::from_str(
        &std::env::var("DATABASE_URL").context("DATABASE_URL required")?,
    )?;
    let admin_options = PgConnectOptions::from_str(
        &std::env::var("CONVT_TEST_DATABASE_URL").context("CONVT_TEST_DATABASE_URL required")?,
    )?;
    ensure!(
        server_options.get_host() == admin_options.get_host()
            && server_options.get_port() == admin_options.get_port(),
        "DB URLs must point to the same disposable Postgres instance"
    );
    let storage = S3Storage::from_env()?;
    let admin = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(admin_options.clone())
        .await?;
    let name = convt_server::ids::new_id("replica").replace('_', "");
    let template = std::env::var("TEST_TEMPLATE_DB").unwrap_or_else(|_| "convt_template".into());
    ensure!(
        template
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && !template.is_empty(),
        "unsafe template database name"
    );
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "create database {name} template {template} owner convt_owner"
    )))
    .execute(&admin)
    .await?;
    let options = server_options.database(&name);
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(4)
        .after_connect(|conn, _| {
            Box::pin(async move {
                sqlx::query("set role convt_owner").execute(conn).await?;
                Ok(())
            })
        })
        .connect_with(admin_options.database(&name))
        .await?;
    let server_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await?;
    let role: String = sqlx::query_scalar("select current_user::text")
        .fetch_one(&server_pool)
        .await?;
    ensure!(
        role == "convt_server",
        "DATABASE_URL must connect as convt_server"
    );
    server_pool.close().await;
    println!("PASS created disposable migrated database {name}; server role checked");
    let evidence = tempfile::Builder::new()
        .prefix("convt-replicas-")
        .tempdir()?
        .keep();
    println!("Evidence: {}", evidence.display());
    let mut owned = Owned {
        children: Vec::new(),
        prefixes: Vec::new(),
        evidence,
    };
    let result =
        tokio::time::timeout(Duration::from_secs(900), run(&pool, &options, &mut owned)).await;
    let stopped = owned.stop().await;
    let mut cleaned = true;
    for prefix in &owned.prefixes {
        cleaned &= matches!(
            tokio::time::timeout(Duration::from_secs(30), storage.delete_prefix(prefix)).await,
            Ok(Ok(()))
        );
    }
    pool.close().await;
    // Never delete account or append-only financial rows. Drop only the entire
    // unique disposable database created by this run, after owned services stop.
    let dropped = sqlx::query(sqlx::AssertSqlSafe(format!(
        "drop database {name} with (force)"
    )))
    .execute(&admin)
    .await;
    admin.close().await;
    stopped?;
    dropped?;
    ensure!(cleaned, "owned S3 fixture-prefix cleanup failed");
    println!("PASS owned processes stopped and only this run's S3 job prefixes removed");
    result.context("acceptance exceeded 900 seconds")??;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn real_http_transport_preserves_upload_length_auth_and_output() -> Result<()> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let server = tokio::spawn(async move {
            for (method, expected) in [
                ("GET", &b""[..]),
                ("POST", &b"{\"input_bytes\":4}"[..]),
                ("PUT", &b"\x00\xffab"[..]),
            ] {
                let (mut stream, _) = listener.accept().await?;
                let mut buffer = Vec::new();
                let split = loop {
                    let mut chunk = [0; 1024];
                    let n = stream.read(&mut chunk).await?;
                    ensure!(n > 0, "request ended before headers");
                    buffer.extend_from_slice(&chunk[..n]);
                    if let Some(split) = buffer.windows(4).position(|part| part == b"\r\n\r\n") {
                        break split + 4;
                    }
                };
                let headers = String::from_utf8(buffer[..split].to_vec())?;
                ensure!(
                    headers.starts_with(&format!("{method} /fixture HTTP/1.1")),
                    "wrong method or route"
                );
                ensure!(
                    headers.contains("Authorization: Bearer private-test-key"),
                    "auth missing"
                );
                if !expected.is_empty() {
                    ensure!(
                        headers.contains(&format!("Content-Length: {}", expected.len())),
                        "exact upload length missing"
                    );
                    ensure!(
                        !headers.to_lowercase().contains("transfer-encoding"),
                        "upload was chunked"
                    );
                    let content_type = if method == "PUT" {
                        "application/octet-stream"
                    } else {
                        "application/json"
                    };
                    ensure!(headers.contains(content_type), "wrong body content type");
                }
                while buffer.len() < split + expected.len() {
                    let mut chunk = [0; 1024];
                    let n = stream.read(&mut chunk).await?;
                    ensure!(n > 0, "request body truncated");
                    buffer.extend_from_slice(&chunk[..n]);
                }
                ensure!(&buffer[split..] == expected, "request bytes changed");
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\n\x00\xffok").await?;
            }
            Ok::<_, anyhow::Error>(())
        });
        let dir = tempfile::tempdir()?;
        let json = dir.path().join("reserve.json");
        let input = dir.path().join("input.png");
        fs::write(&json, b"{\"input_bytes\":4}")?;
        fs::write(&input, b"\x00\xffab")?;
        for (method, body) in [
            ("GET", None),
            ("POST", Some(json.as_path())),
            ("PUT", Some(input.as_path())),
        ] {
            let bytes = http(
                method,
                &format!("http://{address}/fixture"),
                Some("private-test-key"),
                body,
            )
            .await?;
            ensure!(bytes == b"\x00\xffok", "response bytes changed");
        }
        tokio::time::timeout(Duration::from_secs(5), server).await???;
        Ok(())
    }

    #[tokio::test]
    async fn failed_http_response_withholds_signed_url() -> Result<()> {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let address = listener.local_addr()?;
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await?;
            let mut request = [0; 2048];
            stream.read(&mut request).await?;
            stream
                .write_all(
                    b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .await?;
            Ok::<_, anyhow::Error>(())
        });
        let error = http(
            "GET",
            &format!("http://{address}/fixture?secret=must-not-print"),
            None,
            None,
        )
        .await
        .unwrap_err()
        .to_string();
        ensure!(
            error.contains("HTTP GET failed") && !error.contains("must-not-print"),
            "sensitive error leaked"
        );
        server.await??;
        Ok(())
    }
}
