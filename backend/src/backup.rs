use std::{
    io::{Cursor, Read},
    path::{Component, Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, anyhow};
use chrono::Utc;
use flate2::{Compression, read::GzDecoder, write::GzEncoder};
use hmac::{Hmac, Mac};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE, HeaderMap, HeaderValue};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use tar::{Archive, Builder};

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSettings {
    pub enabled: bool,
    /// Optional custom S3 endpoint (e.g. https://<account>.r2.cloudflarestorage.com or http://127.0.0.1:9000)
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    pub prefix: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    /// Use path-style URLs (recommended for MinIO / custom S3 endpoints)
    pub force_path_style: bool,
    /// Optional Cron expression for automatic S3 backup (e.g. "0 3 * * *")
    pub auto_backup_cron: String,
    pub last_backup_at: Option<String>,
    pub last_backup_key: Option<String>,
}

impl Default for BackupSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: String::new(),
            region: "us-east-1".to_string(),
            bucket: String::new(),
            prefix: "aiworker/".to_string(),
            access_key_id: String::new(),
            secret_access_key: String::new(),
            force_path_style: true,
            auto_backup_cron: "0 3 * * *".to_string(),
            last_backup_at: None,
            last_backup_key: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3BackupObject {
    pub key: String,
    pub size_bytes: u64,
    pub last_modified: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreResult {
    pub restored_files: usize,
    pub db_snapshot_saved_to: Option<String>,
}

/// Create a consistent `.tar.gz` snapshot containing:
/// - `db/aiworker.db` (hot SQLite snapshot via `VACUUM INTO`)
/// - `workspace/...` (all user files, reports, and skills)
pub async fn create_backup_archive(
    pool: &SqlitePool,
    workspace_dir: &Path,
) -> anyhow::Result<(String, Vec<u8>)> {
    let ts = Utc::now().format("%Y%m%d-%H%M%S").to_string();
    let filename = format!("aiworker-backup-{ts}.tar.gz");

    // 1. Hot-backup SQLite database into a temporary file using VACUUM INTO
    let tmp_db = std::env::temp_dir().join(format!("aiworker_vac_{}.db", uuid::Uuid::new_v4()));
    let escaped = tmp_db.to_string_lossy().replace('\'', "''");
    let sql = format!("VACUUM INTO '{escaped}'");
    let db_ok = match sqlx::query(&sql).execute(pool).await {
        Ok(_) => true,
        Err(err) => {
            tracing::warn!(error = %err, "SQLite VACUUM INTO failed during backup");
            false
        }
    };

    let ws_dir = workspace_dir.to_path_buf();
    let tmp_db_clone = tmp_db.clone();

    let archive_bytes = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        {
            let mut tar = Builder::new(&mut gz);
            if db_ok && tmp_db_clone.exists() {
                tar.append_path_with_name(&tmp_db_clone, "db/aiworker.db")
                    .context("failed to pack SQLite snapshot")?;
            }
            if ws_dir.exists() {
                tar.append_dir_all("workspace", &ws_dir)
                    .context("failed to pack workspace directory")?;
            }
            tar.finish()?;
        }
        Ok(gz.finish()?)
    })
    .await??;

    let _ = tokio::fs::remove_file(&tmp_db).await;
    Ok((filename, archive_bytes))
}

/// Safely extract a `.tar.gz` backup archive:
/// - Restores `workspace/*` files into `workspace_dir` (strictly blocking path traversal).
/// - Saves `db/aiworker.db` (if present) into `data_dir/aiworker.db.restored` so the live DB isn't corrupted mid-flight.
pub async fn restore_backup_archive(
    archive_bytes: Vec<u8>,
    workspace_dir: &Path,
    data_dir: &Path,
) -> anyhow::Result<RestoreResult> {
    tokio::fs::create_dir_all(workspace_dir).await?;
    tokio::fs::create_dir_all(data_dir).await?;

    let ws_dir = workspace_dir.to_path_buf();
    let d_dir = data_dir.to_path_buf();

    tokio::task::spawn_blocking(move || -> anyhow::Result<RestoreResult> {
        let gz = GzDecoder::new(Cursor::new(archive_bytes));
        let mut archive = Archive::new(gz);

        let mut restored_files = 0usize;
        let mut db_snapshot_saved_to = None;

        for entry_res in archive.entries()? {
            let mut entry = entry_res?;
            let path = entry.path()?.into_owned();

            // Block any path traversal components
            if path.components().any(|c| {
                matches!(
                    c,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            }) {
                continue;
            }

            if path == Path::new("db/aiworker.db") {
                let target = d_dir.join("aiworker.db.restored");
                let mut buf = Vec::new();
                entry.read_to_end(&mut buf)?;
                std::fs::write(&target, buf)?;
                db_snapshot_saved_to = Some(target.display().to_string());
                continue;
            }

            if let Ok(rel) = path.strip_prefix("workspace") {
                if rel.as_os_str().is_empty() {
                    continue;
                }
                let target: PathBuf = ws_dir.join(rel);
                if entry.header().entry_type().is_dir() {
                    std::fs::create_dir_all(&target)?;
                } else if entry.header().entry_type().is_file() {
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    let mut buf = Vec::new();
                    entry.read_to_end(&mut buf)?;
                    std::fs::write(&target, buf)?;
                    restored_files += 1;
                }
            }
        }

        Ok(RestoreResult {
            restored_files,
            db_snapshot_saved_to,
        })
    })
    .await?
}

// ---------------- S3 Compatible Client (AWS SigV4) ----------------

fn build_s3_url(cfg: &BackupSettings, object_key: Option<&str>, query: &str) -> anyhow::Result<(String, String, String)> {
    let bucket = cfg.bucket.trim();
    if bucket.is_empty() {
        return Err(anyhow!("S3 bucket is required"));
    }
    let region = if cfg.region.trim().is_empty() {
        "us-east-1"
    } else {
        cfg.region.trim()
    };

    let (scheme, base_host, base_path) = if cfg.endpoint.trim().is_empty() {
        (
            "https".to_string(),
            format!("s3.{region}.amazonaws.com"),
            String::new(),
        )
    } else {
        let parsed = reqwest::Url::parse(cfg.endpoint.trim())
            .with_context(|| format!("invalid S3 endpoint URL: {}", cfg.endpoint))?;
        let scheme = parsed.scheme().to_string();
        let host = parsed
            .host_str()
            .ok_or_else(|| anyhow!("S3 endpoint missing host"))?
            .to_string();
        let host_with_port = match parsed.port() {
            Some(p) => format!("{host}:{p}"),
            None => host,
        };
        let path = parsed.path().trim_end_matches('/').to_string();
        (scheme, host_with_port, path)
    };

    let key_encoded = object_key
        .map(|k| encode_s3_key(k.trim_start_matches('/')))
        .unwrap_or_default();

    let (host, canonical_uri) = if cfg.force_path_style {
        let uri = if key_encoded.is_empty() {
            format!("{base_path}/{bucket}")
        } else {
            format!("{base_path}/{bucket}/{key_encoded}")
        };
        (base_host, uri)
    } else {
        let host = format!("{bucket}.{base_host}");
        let uri = if key_encoded.is_empty() {
            "/".to_string()
        } else {
            format!("/{key_encoded}")
        };
        (host, uri)
    };

    let url = if query.is_empty() {
        format!("{scheme}://{host}{canonical_uri}")
    } else {
        format!("{scheme}://{host}{canonical_uri}?{query}")
    };

    Ok((url, host, canonical_uri))
}

fn sign_sigv4(
    cfg: &BackupSettings,
    method: &str,
    host: &str,
    canonical_uri: &str,
    canonical_query: &str,
    payload: &[u8],
    content_type: Option<&str>,
) -> anyhow::Result<HeaderMap> {
    let ak = cfg.access_key_id.trim();
    let sk = cfg.secret_access_key.trim();
    if ak.is_empty() || sk.is_empty() {
        return Err(anyhow!("S3 access_key_id and secret_access_key are required"));
    }
    let region = if cfg.region.trim().is_empty() {
        "us-east-1"
    } else {
        cfg.region.trim()
    };

    let now = Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();

    let payload_hash = hex::encode(Sha256::digest(payload));

    let mut canonical_headers = format!(
        "host:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n"
    );
    let mut signed_headers = "host;x-amz-content-sha256;x-amz-date".to_string();

    if let Some(ct) = content_type {
        canonical_headers = format!(
            "content-type:{ct}\nhost:{host}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amz_date}\n"
        );
        signed_headers = "content-type;host;x-amz-content-sha256;x-amz-date".to_string();
    }

    let canonical_request = format!(
        "{method}\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
    );
    let canonical_request_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));

    let scope = format!("{date_stamp}/{region}/s3/aws4_request");
    let string_to_sign = format!("AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{canonical_request_hash}");

    let k_date = hmac_sha256(format!("AWS4{sk}").as_bytes(), date_stamp.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, b"s3");
    let k_signing = hmac_sha256(&k_service, b"aws4_request");
    let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));

    let auth_header = format!(
        "AWS4-HMAC-SHA256 Credential={ak}/{scope}, SignedHeaders={signed_headers}, Signature={signature}"
    );

    let mut headers = HeaderMap::new();
    headers.insert("x-amz-date", HeaderValue::from_str(&amz_date)?);
    headers.insert("x-amz-content-sha256", HeaderValue::from_str(&payload_hash)?);
    headers.insert(AUTHORIZATION, HeaderValue::from_str(&auth_header)?);
    if let Some(ct) = content_type {
        headers.insert(CONTENT_TYPE, HeaderValue::from_str(ct)?);
    }
    Ok(headers)
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn encode_uri_component(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

fn encode_s3_key(key: &str) -> String {
    key.split('/')
        .map(encode_uri_component)
        .collect::<Vec<_>>()
        .join("/")
}

pub async fn push_backup_to_s3(
    cfg: &BackupSettings,
    filename: &str,
    archive_bytes: Vec<u8>,
) -> anyhow::Result<String> {
    let prefix = cfg.prefix.trim().trim_start_matches('/');
    let key = if prefix.is_empty() {
        filename.to_string()
    } else if prefix.ends_with('/') {
        format!("{prefix}{filename}")
    } else {
        format!("{prefix}/{filename}")
    };

    let (url, host, canonical_uri) = build_s3_url(cfg, Some(&key), "")?;
    let headers = sign_sigv4(
        cfg,
        "PUT",
        &host,
        &canonical_uri,
        "",
        &archive_bytes,
        Some("application/gzip"),
    )?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;
    let resp = client
        .put(&url)
        .headers(headers)
        .body(archive_bytes)
        .send()
        .await
        .context("failed to upload backup to S3")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("S3 upload failed ({status}): {text}"));
    }

    Ok(key)
}

pub async fn list_s3_backups(cfg: &BackupSettings) -> anyhow::Result<Vec<S3BackupObject>> {
    let prefix = cfg.prefix.trim().trim_start_matches('/');
    let canonical_query = if prefix.is_empty() {
        "list-type=2".to_string()
    } else {
        format!("list-type=2&prefix={}", encode_uri_component(prefix))
    };

    let (url, host, canonical_uri) = build_s3_url(cfg, None, &canonical_query)?;
    let headers = sign_sigv4(cfg, "GET", &host, &canonical_uri, &canonical_query, b"", None)?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let resp = client
        .get(&url)
        .headers(headers)
        .send()
        .await
        .context("failed to list S3 backups")?;

    let status = resp.status();
    let xml = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(anyhow!("S3 list failed ({status}): {xml}"));
    }

    let mut items = parse_list_objects_xml(&xml);
    items.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));
    Ok(items)
}

pub async fn download_s3_backup(cfg: &BackupSettings, key: &str) -> anyhow::Result<Vec<u8>> {
    let (url, host, canonical_uri) = build_s3_url(cfg, Some(key), "")?;
    let headers = sign_sigv4(cfg, "GET", &host, &canonical_uri, "", b"", None)?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;
    let resp = client
        .get(&url)
        .headers(headers)
        .send()
        .await
        .context("failed to download S3 backup")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(anyhow!("S3 download failed ({status}): {text}"));
    }

    Ok(resp.bytes().await?.to_vec())
}

fn parse_list_objects_xml(xml: &str) -> Vec<S3BackupObject> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<Contents>") {
        let after = &rest[start + 10..];
        let Some(end) = after.find("</Contents>") else {
            break;
        };
        let block = &after[..end];
        let key = extract_xml_tag(block, "Key").unwrap_or_default();
        let size_bytes = extract_xml_tag(block, "Size")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let last_modified = extract_xml_tag(block, "LastModified").unwrap_or_default();
        if !key.is_empty() && key.ends_with(".tar.gz") {
            out.push(S3BackupObject {
                key,
                size_bytes,
                last_modified,
            });
        }
        rest = &after[end + 11..];
    }
    out
}

fn extract_xml_tag(block: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let s = block.find(&open)? + open.len();
    let e = block[s..].find(&close)?;
    Some(block[s..s + e].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[tokio::test]
    async fn creates_and_restores_tar_gz_backup() {
        let tmp = std::env::temp_dir().join(format!("aiworker_bak_{}", uuid::Uuid::new_v4()));
        let ws1 = tmp.join("ws1");
        let ws2 = tmp.join("ws2");
        let data2 = tmp.join("data2");
        tokio::fs::create_dir_all(ws1.join("notes")).await.unwrap();
        tokio::fs::write(ws1.join("notes/hello.md"), "# Backup OK\n")
            .await
            .unwrap();

        let src_db = tmp.join("source.db");
        let pool = db::init_pool(&format!("sqlite://{}", src_db.display()))
            .await
            .unwrap();
        let (filename, bytes) = create_backup_archive(&pool, &ws1).await.unwrap();
        assert!(filename.ends_with(".tar.gz"));
        assert!(!bytes.is_empty());

        let restored = restore_backup_archive(bytes, &ws2, &data2).await.unwrap();
        assert_eq!(restored.restored_files, 1);
        assert!(restored.db_snapshot_saved_to.is_some());
        let content = tokio::fs::read_to_string(ws2.join("notes/hello.md"))
            .await
            .unwrap();
        assert_eq!(content, "# Backup OK\n");

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[test]
    fn parses_s3_list_objects_xml() {
        let xml = r#"<ListBucketResult><Contents><Key>aiworker/aiworker-backup-20261124.tar.gz</Key><LastModified>2026-11-24T10:00:00.000Z</LastModified><Size>2048</Size></Contents></ListBucketResult>"#;
        let list = parse_list_objects_xml(xml);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].size_bytes, 2048);
    }
}
