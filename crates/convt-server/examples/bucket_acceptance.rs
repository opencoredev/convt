//! Acceptance checks for an existing private bucket; never creates provider resources.
//! Source S3_* privately, set CONVT_ACCEPTANCE_ORIGIN to the real web origin, then
//! run `cargo run -p convt-server --example bucket_acceptance` (bound with timeout).
//! HTTP checks cover browser CORS headers; a real browser check remains separate.
use convt_server::storage::{S3Storage, Storage};
use reqwest::{Client, Response, header};
use std::time::Duration;

fn cors(response: &Response, origin: &str) -> anyhow::Result<()> {
    let allowed = response
        .headers()
        .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
        .and_then(|value| value.to_str().ok());
    anyhow::ensure!(
        allowed == Some(origin) || allowed == Some("*"),
        "CORS origin missing"
    );
    Ok(())
}

fn allows(response: &Response, name: header::HeaderName, expected: &str) -> bool {
    response
        .headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.split(',')
                .any(|item| item.trim().eq_ignore_ascii_case(expected) || item.trim() == "*")
        })
}

async fn check(storage: &S3Storage, prefix: &str, origin: &str) -> anyhow::Result<()> {
    let client = Client::builder().timeout(Duration::from_secs(15)).build()?;
    let key = format!("{prefix}delete/input");
    let neighbor = format!("{prefix}keep/input");
    let url = storage.upload_url(&key, 3).await?;
    for method in ["PUT", "GET"] {
        let preflight = client
            .request(reqwest::Method::OPTIONS, &url)
            .header(header::ORIGIN, origin)
            .header(header::ACCESS_CONTROL_REQUEST_METHOD, method)
            .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
            .send()
            .await?;
        anyhow::ensure!(preflight.status().is_success(), "CORS preflight rejected");
        cors(&preflight, origin)?;
        anyhow::ensure!(
            allows(&preflight, header::ACCESS_CONTROL_ALLOW_METHODS, method),
            "CORS method missing"
        );
        anyhow::ensure!(
            allows(
                &preflight,
                header::ACCESS_CONTROL_ALLOW_HEADERS,
                "content-type"
            ),
            "CORS header missing"
        );
    }
    let exact = client
        .put(&url)
        .header(header::ORIGIN, origin)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .body(vec![1_u8, 2, 3])
        .send()
        .await?;
    anyhow::ensure!(exact.status().is_success(), "exact-length PUT rejected");
    cors(&exact, origin)?;
    for body in [vec![1_u8, 2], vec![1_u8, 2, 3, 4]] {
        let wrong = client.put(&url).body(body).send().await?;
        anyhow::ensure!(
            wrong.status() == reqwest::StatusCode::FORBIDDEN,
            "wrong-length PUT was not forbidden"
        );
    }
    anyhow::ensure!(storage.size(&key).await? == 3, "stored size changed");
    println!("PASS exact-length PUT; short and oversized PUT forbidden");

    let get_url = storage.download_url(&key, 60).await?;
    let get = client
        .get(&get_url)
        .header(header::ORIGIN, origin)
        .send()
        .await?;
    anyhow::ensure!(get.status().is_success(), "GET rejected");
    cors(&get, origin)?;
    anyhow::ensure!(
        get.bytes().await?.as_ref() == [1, 2, 3],
        "download bytes differ"
    );
    println!("PASS browser CORS HTTP preflight and PUT/GET response headers");

    let expiring = storage.download_url(&key, 1).await?;
    anyhow::ensure!(
        client.get(&expiring).send().await?.status().is_success(),
        "fresh URL rejected"
    );
    tokio::time::sleep(Duration::from_secs(3)).await;
    anyhow::ensure!(
        client.get(&expiring).send().await?.status() == reqwest::StatusCode::FORBIDDEN,
        "expired URL was not forbidden"
    );
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(header::CONTENT_LENGTH, "3".parse()?);
    let expiring_put = storage
        .bucket
        .presign_put(&key, 1, Some(headers), None)
        .await?;
    anyhow::ensure!(
        client
            .put(&expiring_put)
            .body(vec![1_u8, 2, 3])
            .send()
            .await?
            .status()
            .is_success(),
        "fresh PUT URL rejected"
    );
    tokio::time::sleep(Duration::from_secs(3)).await;
    anyhow::ensure!(
        client
            .put(&expiring_put)
            .body(vec![1_u8, 2, 3])
            .send()
            .await?
            .status()
            == reqwest::StatusCode::FORBIDDEN,
        "expired PUT URL was not forbidden"
    );
    println!("PASS download and upload URL expiry");

    let neighbor_url = storage.upload_url(&neighbor, 1).await?;
    anyhow::ensure!(
        client
            .put(neighbor_url)
            .body(vec![9_u8])
            .send()
            .await?
            .status()
            .is_success(),
        "neighbor PUT rejected"
    );
    storage.delete_prefix(&format!("{prefix}delete/")).await?;
    let deleted = client.get(&get_url).send().await?;
    anyhow::ensure!(
        deleted.status() == reqwest::StatusCode::NOT_FOUND,
        "prefix object still exists"
    );
    anyhow::ensure!(
        storage.size(&neighbor).await? == 1,
        "prefix deletion removed neighbor"
    );
    println!("PASS prefix deletion preserves neighboring prefix");
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let storage = S3Storage::from_env()?;
    let origin = std::env::var("CONVT_ACCEPTANCE_ORIGIN")
        .map_err(|_| anyhow::anyhow!("CONVT_ACCEPTANCE_ORIGIN is required"))?;
    let parsed =
        reqwest::Url::parse(&origin).map_err(|_| anyhow::anyhow!("invalid acceptance origin"))?;
    anyhow::ensure!(
        ["http", "https"].contains(&parsed.scheme())
            && parsed.origin().ascii_serialization() == origin,
        "acceptance origin must contain only scheme, host and optional port"
    );
    let prefix = format!("convt-acceptance-{}/", convt_server::ids::new_id("bucket"));
    let result =
        tokio::time::timeout(Duration::from_secs(90), check(&storage, &prefix, &origin)).await;
    // Always clean only this run's unique prefix, even after a failed check.
    let cleanup =
        tokio::time::timeout(Duration::from_secs(30), storage.delete_prefix(&prefix)).await;
    anyhow::ensure!(
        matches!(cleanup, Ok(Ok(()))),
        "acceptance prefix cleanup failed"
    );
    println!("PASS acceptance prefix cleanup");
    // Never print request errors: they can contain signed URLs or credentials.
    anyhow::ensure!(
        matches!(result, Ok(Ok(()))),
        "bucket acceptance failed (request details withheld)"
    );
    Ok(())
}
