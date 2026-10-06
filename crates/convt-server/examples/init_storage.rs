//! Local-only bucket setup. Never called by the API or production worker.
use s3::{
    Bucket, Region,
    bucket_ops::BucketConfiguration,
    creds::Credentials,
    serde_types::{
        AbortIncompleteMultipartUpload, BucketLifecycleConfiguration, Expiration, LifecycleRule,
    },
};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var("CONVT_STORAGE_DEV").as_deref() == Ok("1"),
        "local storage setup requires CONVT_STORAGE_DEV=1"
    );
    let region = Region::Custom {
        region: std::env::var("S3_REGION")?,
        endpoint: std::env::var("S3_ENDPOINT")?,
    };
    let credentials = Credentials::new(
        Some(&std::env::var("S3_ACCESS_KEY")?),
        Some(&std::env::var("S3_SECRET_KEY")?),
        None,
        None,
        None,
    )?;
    let created = Bucket::create_with_path_style(
        &std::env::var("S3_BUCKET")?,
        region,
        credentials,
        BucketConfiguration::default(),
    )
    .await?;
    anyhow::ensure!(
        (200..300).contains(&created.response_code) || created.response_code == 409,
        "bucket creation HTTP {}",
        created.response_code
    );
    let storage = convt_server::storage::S3Storage::from_env()?;
    let lifecycle = BucketLifecycleConfiguration::new(vec![
        LifecycleRule::builder("Enabled")
            .id("convt-24h")
            .expiration(Expiration::new(None, Some(1), None))
            .abort_incomplete_multipart_upload(AbortIncompleteMultipartUpload::new(Some(1)))
            .build(),
    ]);
    let response = storage.bucket.put_bucket_lifecycle(lifecycle).await?;
    anyhow::ensure!(
        (200..300).contains(&response.status_code()),
        "lifecycle setup failed"
    );
    println!("local bucket initialized with 1-day expiry");
    Ok(())
}
