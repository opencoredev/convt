//! Object storage boundary for Railway buckets and local MinIO.
use async_trait::async_trait;
use s3::{Bucket, Region, creds::Credentials};
use std::path::Path;
#[async_trait]
pub trait Storage: Send + Sync {
    async fn upload_url(&self, key: &str, bytes: i64) -> anyhow::Result<String>;
    async fn download_url(&self, key: &str, seconds: u32) -> anyhow::Result<String>;
    async fn size(&self, key: &str) -> anyhow::Result<i64>;
    async fn seal(&self, from: &str, to: &str) -> anyhow::Result<()>;
    async fn download(&self, key: &str, path: &Path) -> anyhow::Result<()>;
    async fn upload(&self, key: &str, path: &Path) -> anyhow::Result<()>;
    async fn delete_prefix(&self, prefix: &str) -> anyhow::Result<()>;
    async fn delete(&self, key: &str) -> anyhow::Result<()>;
}
pub struct S3Storage {
    pub bucket: Box<Bucket>,
}
impl S3Storage {
    pub fn from_env() -> anyhow::Result<Self> {
        let required =
            |name: &str| std::env::var(name).map_err(|_| anyhow::anyhow!("{name} is required"));
        let region = Region::Custom {
            region: std::env::var("S3_REGION").unwrap_or_else(|_| "auto".into()),
            endpoint: required("S3_ENDPOINT")?,
        };
        let credentials = Credentials::new(
            Some(&required("S3_ACCESS_KEY")?),
            Some(&required("S3_SECRET_KEY")?),
            None,
            None,
            None,
        )?;
        let path_style = match std::env::var("S3_PATH_STYLE") {
            Ok(value) => parse_path_style(Some(&value))?,
            Err(std::env::VarError::NotPresent) => parse_path_style(None)?,
            Err(_) => anyhow::bail!("S3_PATH_STYLE must be true or false"),
        };
        let bucket = configured_bucket(&required("S3_BUCKET")?, region, credentials, path_style)?;
        Ok(Self { bucket })
    }
}
fn parse_path_style(value: Option<&str>) -> anyhow::Result<bool> {
    match value {
        None | Some("false") => Ok(false),
        Some("true") => Ok(true),
        Some(_) => anyhow::bail!("S3_PATH_STYLE must be true or false"),
    }
}

fn configured_bucket(
    name: &str,
    region: Region,
    credentials: Credentials,
    path_style: bool,
) -> anyhow::Result<Box<Bucket>> {
    let bucket = Bucket::new(name, region, credentials)?;
    Ok(if path_style {
        bucket.with_path_style()
    } else {
        bucket
    })
}

fn success(code: u16) -> anyhow::Result<()> {
    anyhow::ensure!((200..300).contains(&code), "object storage HTTP {code}");
    Ok(())
}
#[async_trait]
impl Storage for S3Storage {
    async fn upload_url(&self, key: &str, bytes: i64) -> anyhow::Result<String> {
        anyhow::ensure!(
            bytes > 0 && bytes <= crate::jobs::MAX_FILE_BYTES,
            "invalid upload size"
        );
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            axum::http::header::CONTENT_LENGTH,
            bytes.to_string().parse()?,
        );
        Ok(self
            .bucket
            .presign_put(key, 900, Some(headers), None)
            .await?)
    }
    async fn download_url(&self, key: &str, seconds: u32) -> anyhow::Result<String> {
        Ok(self.bucket.presign_get(key, seconds, None).await?)
    }
    async fn size(&self, key: &str) -> anyhow::Result<i64> {
        let (h, c) = self.bucket.head_object(key).await?;
        success(c)?;
        h.content_length
            .ok_or_else(|| anyhow::anyhow!("missing content length"))
    }
    async fn seal(&self, from: &str, to: &str) -> anyhow::Result<()> {
        success(self.bucket.copy_object_internal(from, to).await?)
    }
    async fn download(&self, key: &str, path: &Path) -> anyhow::Result<()> {
        let mut file = tokio::fs::File::create(path).await?;
        success(self.bucket.get_object_to_writer(key, &mut file).await?)?;
        tokio::io::AsyncWriteExt::flush(&mut file).await?;
        Ok(())
    }
    async fn upload(&self, key: &str, path: &Path) -> anyhow::Result<()> {
        let mut file = tokio::fs::File::open(path).await?;
        let r = self.bucket.put_object_stream(&mut file, key).await?;
        success(r.status_code())
    }
    async fn delete(&self, key: &str) -> anyhow::Result<()> {
        success(self.bucket.delete_object(key).await?.status_code())
    }
    async fn delete_prefix(&self, prefix: &str) -> anyhow::Result<()> {
        for page in self.bucket.list(prefix.to_owned(), None).await? {
            for object in page.contents {
                self.delete(&object.key).await?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn path_style_defaults_to_virtual_hosted_and_rejects_invalid_values() {
        assert!(!parse_path_style(None).unwrap());
        assert!(!parse_path_style(Some("false")).unwrap());
        assert!(parse_path_style(Some("true")).unwrap());
        for invalid in ["", "1", "0", "TRUE", "False", " true", "false ", "yes"] {
            assert!(parse_path_style(Some(invalid)).is_err());
        }
    }

    #[tokio::test]
    async fn upload_urls_use_configured_addressing_and_sign_reserved_length() {
        for (setting, host, path) in [
            (None, "test.storage.example.com", "/job/input"),
            (Some("false"), "test.storage.example.com", "/job/input"),
            (Some("true"), "storage.example.com", "/test/job/input"),
        ] {
            let bucket = configured_bucket(
                "test",
                Region::Custom {
                    region: "auto".into(),
                    endpoint: "https://storage.example.com:9443".into(),
                },
                Credentials::new(Some("test"), Some("test-secret"), None, None, None).unwrap(),
                parse_path_style(setting).unwrap(),
            )
            .unwrap();
            let storage = S3Storage { bucket };
            let url =
                reqwest::Url::parse(&storage.upload_url("job/input", 1).await.unwrap()).unwrap();
            assert_eq!(url.host_str(), Some(host));
            assert_eq!(url.port(), Some(9443));
            assert_eq!(url.path(), path);
            let query = url
                .query_pairs()
                .collect::<std::collections::HashMap<_, _>>();
            assert_eq!(
                query.get("X-Amz-SignedHeaders").unwrap(),
                "content-length;host"
            );
            assert_eq!(query.get("X-Amz-Expires").unwrap(), "900");
            let other =
                reqwest::Url::parse(&storage.upload_url("job/input", 2).await.unwrap()).unwrap();
            let other_query = other
                .query_pairs()
                .collect::<std::collections::HashMap<_, _>>();
            assert_ne!(
                query.get("X-Amz-Signature"),
                other_query.get("X-Amz-Signature")
            );
            for bytes in [0, -1, crate::jobs::MAX_FILE_BYTES + 1] {
                assert!(storage.upload_url("job/input", bytes).await.is_err());
            }
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn s3_rejects_uploads_larger_than_signed_reservation() {
        if std::env::var("CONVT_TEST_STORAGE").as_deref() != Ok("1") {
            return;
        }
        let storage = S3Storage::from_env().unwrap();
        let key = format!("security-test-{}/input", crate::ids::new_id("upload"));
        let url = storage.upload_url(&key, 1).await.unwrap();
        let client = reqwest::Client::new();
        let exact = client.put(&url).body(vec![1_u8]).send().await.unwrap();
        assert!(exact.status().is_success());
        let oversized = client.put(&url).body(vec![1_u8, 2]).send().await.unwrap();
        assert_eq!(oversized.status(), reqwest::StatusCode::FORBIDDEN);
        assert_eq!(storage.size(&key).await.unwrap(), 1);
        storage.delete(&key).await.unwrap();
    }
}
