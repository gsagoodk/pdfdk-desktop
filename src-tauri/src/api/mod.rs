// API client for PDF.dk
// Handles the tools catalogue, file upload, job polling and download.

use crate::config::{CatalogCache, ToolDefinition};
use reqwest::{multipart, Client};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;
use tokio::fs;
use tracing::{debug, info};
use uuid::Uuid;

const POLL_INTERVAL: Duration = Duration::from_secs(2);
const MAX_POLL_ATTEMPTS: u32 = 300; // 10 minutes max

/// Base URL of the API. `PDFDK_API_BASE=https://dev.pdf.dk/api` at launch
/// points a build at the dev server; production is the default.
pub fn api_base() -> String {
    // Another server (a test server) only in development builds, and only over https: in the app
    // people install, nothing in the environment can send the login and token elsewhere.
    #[cfg(debug_assertions)]
    if let Ok(b) = std::env::var("PDFDK_API_BASE") {
        if b.trim().starts_with("https://") {
            return b.trim().trim_end_matches('/').to_string();
        }
    }
    "https://pdf.dk/api".to_string()
}

/// Origin of the website, derived from the API base (for links).
pub fn site_base() -> String {
    api_base().trim_end_matches("/api").to_string()
}

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Job failed: {0}")]
    JobFailed(String),
    #[error("Job timeout")]
    Timeout,
    #[error("Server error: {0}")]
    ServerError(String),
    #[error("Unauthorized - please login again")]
    Unauthorized,
    #[error("Monthly job limit exceeded")]
    JobLimitExceeded,
    #[error("The server asked for a pause ({0} s) — retrying")]
    RateLimited(u64),
    #[error("File too large for your plan (max {0} MB)")]
    FileTooLarge(i32),
    /// One Pro account, one computer: pdf.dk says this account's Pro is on another computer.
    #[error("PDF.dk Pro bruges på en anden computer ({0}). Vælg »Brug Pro her i stedet« i PDF.dk Flow.")]
    DeviceElsewhere(String),
}

impl ApiError {
    /// Worth another attempt: the network hiccupped or the server had a bad moment.
    /// Everything the user or their plan caused (401, limits, validation) is final.
    pub fn is_transient(&self) -> bool {
        match self {
            ApiError::Network(e) => e.is_timeout() || e.is_connect() || e.is_request() || e.status().map_or(true, |s| s.is_server_error()),
            ApiError::ServerError(msg) => {
                let m = msg.to_ascii_lowercase();
                m.contains("502") || m.contains("503") || m.contains("504") || m.contains("gateway") || m.contains("timeout")
            }
            // 429 from the API rate limiter: wait what Retry-After says, then go on (0.3.4)
            ApiError::RateLimited(_) => true,
            _ => false,
        }
    }
}

/// Seconds the server asked us to wait (Retry-After), clamped to 5–120 s. None = not rate limited.
fn retry_after(response: &reqwest::Response) -> Option<u64> {
    if response.status() != reqwest::StatusCode::TOO_MANY_REQUESTS {
        return None;
    }
    let secs = response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(30);
    Some(secs.clamp(5, 120))
}

/// Run `op` up to `attempts` times with backoff (2 s, 5 s, 10 s) while the error is transient.
pub async fn with_retry<T, F, Fut>(what: &str, attempts: u32, mut op: F) -> Result<T, ApiError>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, ApiError>>,
{
    let waits = [2u64, 5, 10];
    let mut n = 0;
    loop {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) if e.is_transient() && n + 1 < attempts => {
                let wait = match e {
                    ApiError::RateLimited(secs) => secs + 1,
                    _ => waits[(n as usize).min(waits.len() - 1)],
                };
                crate::add_log(&format!("{} failed ({}), retrying in {} s ({}/{})", what, e, wait, n + 1, attempts - 1));
                tokio::time::sleep(Duration::from_secs(wait)).await;
                n += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadResponse {
    pub success: bool,
    pub message: Option<String>,
    pub data: Option<UploadData>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadData {
    pub job_uuid: String,
    pub status: String,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStatusResponse {
    pub success: bool,
    pub data: Option<UsageStatusData>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageStatusData {
    pub plan: String,
    pub limit: i32,
    pub used: i32,
    #[serde(default)]
    pub is_unlimited: bool,
    #[serde(default)]
    pub is_authenticated: bool,
    #[serde(default)]
    pub batch_upload: bool,
    #[serde(default)]
    pub max_file_size_mb: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStatusResponse {
    pub success: bool,
    pub message: Option<String>,
    pub data: Option<JobStatusData>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStatusData {
    pub uuid: String,
    pub status: String,
    pub progress: Option<u8>,
    pub output_path: Option<String>,
    pub output_filename: Option<String>,
    pub error: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct CatalogResponse {
    success: bool,
    data: Option<CatalogData>,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CatalogData {
    version: Option<String>,
    generated_at: Option<String>,
    tools: Vec<ToolDefinition>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JobStatus {
    Queued,
    Processing,
    Completed,
    Failed,
    Unknown(String),
}

impl From<&str> for JobStatus {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "queued" | "pending" => JobStatus::Queued,
            "processing" => JobStatus::Processing,
            "completed" | "done" | "downloaded" => JobStatus::Completed,
            "failed" | "error" => JobStatus::Failed,
            other => JobStatus::Unknown(other.to_string()),
        }
    }
}

fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("tif") | Some("tiff") => "image/tiff",
        Some("avif") => "image/avif",
        Some("heic") | Some("heif") => "image/heic",
        Some("svg") => "image/svg+xml",
        Some("eps") | Some("ai") => "application/postscript",
        Some("eml") => "message/rfc822",
        Some("msg") => "application/vnd.ms-outlook",
        Some("ppt") => "application/vnd.ms-powerpoint",
        Some("pptx") => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        Some("stl") => "model/stl",
        Some("step") | Some("stp") => "model/step",
        _ => "application/octet-stream",
    }
}

/// Every request says who we are (`X-PDFDK-Client: desktop/<version>`), so the site can tag
/// jobs from the app and Admin › Jobs can filter on them (0.4.5).
pub const CLIENT_HEADER: &str = "X-PDFDK-Client";
pub fn client_id() -> String {
    format!("desktop/{}", env!("CARGO_PKG_VERSION"))
}
pub fn default_headers() -> reqwest::header::HeaderMap {
    let mut h = reqwest::header::HeaderMap::new();
    if let Ok(v) = reqwest::header::HeaderValue::from_str(&client_id()) {
        h.insert(CLIENT_HEADER, v);
    }
    // Which computer asks (one Pro account, one computer – shared with PDF.dk Desktop on the same
    // machine), and its name for the account page.
    if let Ok(v) = reqwest::header::HeaderValue::from_str(&device_id()) {
        h.insert("X-PDFDK-Device", v);
    }
    if let Ok(v) = reqwest::header::HeaderValue::from_bytes(device_name().as_bytes()) {
        h.insert("X-PDFDK-Device-Name", v);
    }
    h
}

/// This computer, as pdf.dk sees it: a hash of the system's machine id (never the id itself). The
/// same recipe as PDF.dk Desktop, so both apps on one computer count as one.
pub fn device_id() -> String {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(|| match machine_uid::get() {
        Ok(uid) if !uid.trim().is_empty() => {
            use sha2::Digest;
            let d = sha2::Sha256::digest(format!("pdfdk-device:{}", uid.trim()).as_bytes());
            d.iter().take(16).map(|b| format!("{b:02x}")).collect()
        }
        _ => String::new(),
    })
    .clone()
}

/// The computer's name as its owner knows it ("Gurmukhs MacBook Pro").
pub fn device_name() -> String {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| {
        #[cfg(target_os = "macos")]
        if let Ok(o) = std::process::Command::new("/usr/sbin/scutil").args(["--get", "ComputerName"]).output() {
            let n = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !n.is_empty() {
                return n;
            }
        }
        std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")).unwrap_or_else(|_| "Computer".into())
    })
    .chars()
    .filter(|c| !c.is_control())
    .take(80)
    .collect()
}

/// A field of pdf.dk's answer, at the top or under "data" / "error".
pub fn field<'a>(v: &'a serde_json::Value, key: &str) -> &'a serde_json::Value {
    [&v[key], &v["data"][key], &v["error"][key]].into_iter().find(|x| !x.is_null()).unwrap_or(&serde_json::Value::Null)
}
/// A plain client with the identity header (auth calls use this).
pub fn http_client() -> Client {
    Client::builder().default_headers(default_headers()).build().unwrap_or_else(|_| Client::new())
}

/// PDF.dk API Client
pub struct PdfDkClient {
    client: Client,
    auth_token: Option<String>,
    session_id: String,
}

impl PdfDkClient {
    pub fn new(auth_token: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(600))
            .default_headers(default_headers())
            .build()
            .expect("Failed to create HTTP client");
        let session_id = Uuid::new_v4().to_string();
        Self { client, auth_token, session_id }
    }

    fn with_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let req = req.header("X-Session-ID", &self.session_id).header("Accept", "application/json");
        match &self.auth_token {
            Some(token) => req.header("Authorization", format!("Bearer {}", token)),
            None => req,
        }
    }

    /// GET /api/tools — the catalogue the website maintains.
    pub async fn fetch_catalog(&self) -> Result<CatalogCache, ApiError> {
        let url = format!("{}/tools", api_base());
        let response = self.with_auth(self.client.get(&url)).send().await?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(ApiError::ServerError(format!("Server returned {} for /tools", status)));
        }
        let parsed: CatalogResponse = serde_json::from_str(&body)
            .map_err(|e| ApiError::ServerError(format!("Failed to parse catalogue: {}", e)))?;
        if !parsed.success {
            return Err(ApiError::ServerError(parsed.message.unwrap_or_else(|| "catalogue refused".into())));
        }
        let data = parsed.data.ok_or_else(|| ApiError::ServerError("catalogue empty".into()))?;
        Ok(CatalogCache { version: data.version, fetched_at: data.generated_at, tools: data.tools })
    }

    /// Upload a file to a tool endpoint. Returns the job UUID for polling.
    pub async fn process_file(
        &self,
        file_path: &Path,
        endpoint_id: &str,
        file_field: &str,
        options: &serde_json::Value,
    ) -> Result<String, ApiError> {
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();

        info!("Uploading {} to /{}", file_name, endpoint_id);

        // Streamed from disk (a 500 MB print file must not sit in RAM) with the
        // length known, so the server sees a normal multipart upload.
        let len = fs::metadata(file_path).await?.len();
        let file = fs::File::open(file_path).await?;
        let stream = tokio_util::io::ReaderStream::new(file);
        let mut form = multipart::Form::new().part(
            file_field.to_string(),
            multipart::Part::stream_with_length(reqwest::Body::wrap_stream(stream), len)
                .file_name(file_name.clone())
                .mime_str(mime_for(file_path))
                .unwrap(),
        );

        if let Some(obj) = options.as_object() {
            for (key, value) in obj {
                let text = match value {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Bool(b) => if *b { "1".into() } else { "0".into() },
                    serde_json::Value::Null => continue,
                    other => other.to_string(),
                };
                form = form.text(key.clone(), text);
            }
        }

        let url = format!("{}/{}", api_base(), endpoint_id);
        debug!("POST {}", url);

        // Uploads get their own, long timeout: the client default (10 min) is for
        // polling. 500 MB on a slow office uplink can take a while; the server
        // allows 1 GB / 600 s per request.
        let response = self
            .with_auth(self.client.post(&url))
            .timeout(Duration::from_secs(3600))
            .multipart(form)
            .send()
            .await?;
        self.job_uuid_from(response).await
    }

    /// Status handling shared by the single- and multi-file uploads.
    async fn job_uuid_from(&self, response: reqwest::Response) -> Result<String, ApiError> {
        let status = response.status();

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ApiError::Unauthorized);
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(match retry_after(&response) {
                Some(secs) => ApiError::RateLimited(secs),
                None => ApiError::JobLimitExceeded,
            });
        }
        if status == reqwest::StatusCode::PAYLOAD_TOO_LARGE {
            return Err(ApiError::FileTooLarge(100));
        }

        let body = response.text().await.unwrap_or_default();
        if status == reqwest::StatusCode::CONFLICT {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) {
                if field(&v, "code").as_str() == Some("device_limit") {
                    let name = field(&v, "active")["name"].as_str().unwrap_or("ukendt").to_string();
                    return Err(ApiError::DeviceElsewhere(name));
                }
            }
        }
        info!("Upload response {}: {}", status, body.chars().take(300).collect::<String>());

        if !status.is_success() {
            // Laravel validation errors carry a readable message
            let msg = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(|s| s.to_string()))
                .unwrap_or_else(|| format!("Server returned {}", status));
            return Err(ApiError::ServerError(msg));
        }

        let upload_response: UploadResponse = serde_json::from_str(&body)
            .map_err(|e| ApiError::ServerError(format!("Failed to parse response: {}", e)))?;

        if !upload_response.success {
            return Err(ApiError::ServerError(
                upload_response.error.or(upload_response.message).unwrap_or_else(|| "Unknown error".to_string()),
            ));
        }

        upload_response
            .data
            .map(|d| d.job_uuid)
            .ok_or(ApiError::ServerError("No job UUID returned from server".to_string()))
    }

    /// Several files in ONE request (tools whose field is "files[]"): the server combines
    /// them, e.g. images → one PDF, in the order given.
    pub async fn process_files_multi(
        &self,
        file_paths: &[PathBuf],
        endpoint_id: &str,
        file_field: &str,
        options: &serde_json::Value,
    ) -> Result<String, ApiError> {
        let mut form = multipart::Form::new();
        for file_path in file_paths {
            let file_name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("file").to_string();
            let len = fs::metadata(file_path).await?.len();
            let file = fs::File::open(file_path).await?;
            let stream = tokio_util::io::ReaderStream::new(file);
            form = form.part(
                file_field.to_string(),
                multipart::Part::stream_with_length(reqwest::Body::wrap_stream(stream), len)
                    .file_name(file_name)
                    .mime_str(mime_for(file_path))
                    .unwrap(),
            );
        }
        if let Some(obj) = options.as_object() {
            for (key, value) in obj {
                let text = match value {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Bool(b) => if *b { "1".into() } else { "0".into() },
                    serde_json::Value::Null => continue,
                    other => other.to_string(),
                };
                form = form.text(key.clone(), text);
            }
        }
        let url = format!("{}/{}", api_base(), endpoint_id);
        info!("Uploading {} files to /{}", file_paths.len(), endpoint_id);
        let response = self.with_auth(self.client.post(&url)).timeout(Duration::from_secs(3600)).multipart(form).send().await?;
        self.job_uuid_from(response).await
    }

    /// Poll job status until completion
    pub async fn poll_job(&self, uuid: &str) -> Result<JobStatusData, ApiError> {
        let url = format!("{}/jobs/{}", api_base(), uuid);
        let mut attempts = 0;

        loop {
            attempts += 1;
            if attempts > MAX_POLL_ATTEMPTS {
                return Err(ApiError::Timeout);
            }

            let response = self.with_auth(self.client.get(&url)).send().await?;
            if response.status() == reqwest::StatusCode::UNAUTHORIZED {
                return Err(ApiError::Unauthorized);
            }
            if let Some(secs) = retry_after(&response) {
                return Err(ApiError::RateLimited(secs));
            }
            let body = response.text().await.unwrap_or_default();

            let job_response: JobStatusResponse = serde_json::from_str(&body)
                .map_err(|e| ApiError::ServerError(format!("Failed to parse poll response: {}", e)))?;

            if !job_response.success {
                let msg = job_response.error.or(job_response.message).unwrap_or_default();
                if msg.to_lowercase().contains("unauthorized") {
                    return Err(ApiError::Unauthorized);
                }
                return Err(ApiError::ServerError(msg));
            }

            if let Some(job) = job_response.data {
                match JobStatus::from(job.status.as_str()) {
                    JobStatus::Completed => {
                        info!("Job {} completed", uuid);
                        return Ok(job);
                    }
                    JobStatus::Failed => {
                        return Err(ApiError::JobFailed(
                            job.error_message.or(job.error).unwrap_or_else(|| "Unknown error".to_string()),
                        ));
                    }
                    _ => tokio::time::sleep(POLL_INTERVAL).await,
                }
            } else {
                tokio::time::sleep(POLL_INTERVAL).await;
            }
        }
    }

    /// Download the completed file
    pub async fn download_result(&self, uuid: &str, output_path: &Path) -> Result<(), ApiError> {
        let url = format!("{}/jobs/{}/download", api_base(), uuid);
        info!("Downloading result to: {:?}", output_path);

        let mut request = self.client.get(&url)
            .header("X-Session-ID", &self.session_id)
            .header("Accept", "application/octet-stream");
        if let Some(ref token) = self.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }
        let response = request.send().await?;

        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ApiError::Unauthorized);
        }
        if let Some(secs) = retry_after(&response) {
            return Err(ApiError::RateLimited(secs));
        }
        if !response.status().is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ApiError::ServerError(format!("Download failed: {}", body)));
        }

        let bytes = response.bytes().await?;
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(output_path, bytes).await?;
        info!("Downloaded {} bytes to {:?}", output_path.metadata()?.len(), output_path);
        Ok(())
    }

    /// Get usage status for the current user
    pub async fn get_usage_status(&self) -> Result<UsageStatusData, ApiError> {
        let url = format!("{}/settings/usage-status", api_base());
        let response = self.with_auth(self.client.get(&url)).send().await?;
        if response.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(ApiError::Unauthorized);
        }
        let body = response.text().await.unwrap_or_default();
        let usage_response: UsageStatusResponse = serde_json::from_str(&body)
            .map_err(|e| ApiError::ServerError(format!("Failed to parse usage response: {}", e)))?;
        if !usage_response.success {
            return Err(ApiError::ServerError(usage_response.message.unwrap_or_else(|| "Unknown error".to_string())));
        }
        usage_response.data.ok_or(ApiError::ServerError("No usage data returned".to_string()))
    }
}
