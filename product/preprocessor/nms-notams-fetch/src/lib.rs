use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{bail, Context};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use chrono::{DateTime, Utc};
use flate2::read::GzDecoder;
use preprocessor_live_feeds::nms_initial_load::{
    parse_nms_initial_load, NmsInitialLoadParseResult, NmsNotamClassification,
};
use preprocessor_live_feeds::StructuredNotamRecord;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub mod collector;

const NMS_HTTP_ATTEMPTS: usize = 5;
const NMS_HTTP_RETRY_DELAY: Duration = Duration::from_secs(2);
const NMS_JSON_RESPONSE_LIMIT: u64 = 512 * 1024 * 1024;
const NMS_HTTP_ERROR_PREVIEW_LIMIT: u64 = 4 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NmsConfig {
    pub source_environment: String,
    pub api_base_url: String,
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
}

impl NmsConfig {
    pub fn from_path(path: &Path) -> anyhow::Result<Self> {
        let bytes = fs::read(path)
            .with_context(|| format!("failed to read NMS config {}", path.display()))?;
        let config = serde_json::from_slice::<Self>(&bytes)
            .with_context(|| format!("failed to parse NMS config {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> anyhow::Result<()> {
        validate_https_url(&self.api_base_url, "apiBaseUrl")?;
        validate_https_url(&self.token_url, "tokenUrl")?;
        if !matches!(self.source_environment.as_str(), "staging" | "production") {
            bail!("NMS sourceEnvironment must be staging or production");
        }
        validate_secret_value(&self.client_id, "clientId")?;
        validate_secret_value(&self.client_secret, "clientSecret")?;
        if self.client_id.contains(':') {
            bail!("NMS clientId cannot contain ':'");
        }
        Ok(())
    }
}

pub trait InitialLoadSource {
    fn capture_source(&self) -> InitialLoadCaptureSource;

    fn fetch_classification(
        &mut self,
        classification: NmsNotamClassification,
        output_gzip_path: &Path,
    ) -> anyhow::Result<()>;
}

pub trait NmsApiSource: InitialLoadSource {
    fn fetch_updates(
        &mut self,
        classification: NmsNotamClassification,
        last_updated_since: DateTime<Utc>,
    ) -> anyhow::Result<Vec<String>>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitialLoadCaptureSource {
    pub environment: String,
    pub api_base_url: Option<String>,
}

pub struct NmsClient {
    config: NmsConfig,
    http: NmsHttpClient,
    access_token: Option<CachedAccessToken>,
}

struct CachedAccessToken {
    value: String,
    refresh_at: Instant,
}

#[derive(Deserialize)]
struct NotamUpdateResponse {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    errors: Vec<serde_json::Value>,
    data: Option<NotamUpdateResponseData>,
}

#[derive(Deserialize)]
struct NotamUpdateResponseData {
    #[serde(default)]
    aixm: Vec<String>,
}

impl NmsClient {
    pub fn new(config: NmsConfig) -> Self {
        Self {
            config,
            http: NmsHttpClient::new(NMS_HTTP_ATTEMPTS, NMS_HTTP_RETRY_DELAY),
            access_token: None,
        }
    }

    fn access_token(&mut self) -> anyhow::Result<String> {
        if let Some(token) = &self.access_token {
            if Instant::now() < token.refresh_at {
                return Ok(token.value.clone());
            }
        }

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            #[serde(
                default = "default_token_lifetime_seconds",
                deserialize_with = "deserialize_u64_or_string"
            )]
            expires_in: u64,
        }

        let authorization = format!(
            "Basic {}",
            BASE64_STANDARD.encode(format!(
                "{}:{}",
                self.config.client_id, self.config.client_secret
            ))
        );
        let bytes =
            self.http
                .request_bytes("OAuth token request", NMS_JSON_RESPONSE_LIMIT, |agent| {
                    agent
                        .post(self.config.token_url.as_str())
                        .header("Authorization", authorization.as_str())
                        .config()
                        .http_status_as_error(false)
                        .max_redirects(0)
                        .build()
                        .send_form([("grant_type", "client_credentials")])
                })?;
        let response = serde_json::from_slice::<TokenResponse>(&bytes)
            .context("NMS OAuth token response was not valid JSON")?;
        if response.access_token.trim().is_empty() {
            bail!("NMS OAuth token response contained an empty access token");
        }
        let refresh_after = response.expires_in.saturating_sub(60).max(60);
        self.access_token = Some(CachedAccessToken {
            value: response.access_token.clone(),
            refresh_at: Instant::now() + Duration::from_secs(refresh_after),
        });
        Ok(response.access_token)
    }

    fn request_content_url(
        &self,
        token: &str,
        classification: NmsNotamClassification,
    ) -> anyhow::Result<ResolvedContentUrl> {
        #[derive(Deserialize)]
        struct InitialLoadResponse {
            data: InitialLoadResponseData,
        }
        #[derive(Deserialize)]
        struct InitialLoadResponseData {
            url: String,
        }

        let endpoint = format!(
            "{}/notams/il/{}",
            self.config.api_base_url.trim_end_matches('/'),
            classification.api_name()
        );
        let authorization = format!("Bearer {token}");
        let bytes = self.http.request_bytes(
            "Initial Load URL request",
            NMS_JSON_RESPONSE_LIMIT,
            |agent| {
                agent
                    .get(endpoint.as_str())
                    .query("allowRedirect", "false")
                    .header("Authorization", authorization.as_str())
                    .config()
                    .http_status_as_error(false)
                    .max_redirects(0)
                    .build()
                    .call()
            },
        )?;
        let response = serde_json::from_slice::<InitialLoadResponse>(&bytes)
            .context("NMS Initial Load URL response was not valid JSON")?;
        resolve_content_url(&self.config.api_base_url, &response.data.url)
    }

    fn download_content(
        &self,
        token: &str,
        content: &ResolvedContentUrl,
        output_path: &Path,
    ) -> anyhow::Result<()> {
        let authorization = format!("Bearer {token}");
        self.http.request(
            "Initial Load content download",
            |agent| {
                let request = agent.get(content.url.as_str());
                let request = if content.send_bearer {
                    request.header("Authorization", authorization.as_str())
                } else {
                    request
                };
                let config = request.config().http_status_as_error(false);
                let config = if content.send_bearer {
                    config.max_redirects(0)
                } else {
                    config
                };
                config.build().call()
            },
            |response| {
                let output = File::create(output_path)
                    .with_context(|| format!("failed to create {}", output_path.display()))?;
                let mut output = BufWriter::new(output);
                let mut body = response.body_mut().as_reader();
                std::io::copy(&mut body, &mut output)
                    .with_context(|| format!("failed to download {}", output_path.display()))?;
                output
                    .flush()
                    .with_context(|| format!("failed to flush {}", output_path.display()))
            },
        )
    }

    fn request_updates(
        &self,
        token: &str,
        classification: NmsNotamClassification,
        last_updated_since: DateTime<Utc>,
    ) -> anyhow::Result<Vec<String>> {
        let endpoint = format!("{}/notams", self.config.api_base_url.trim_end_matches('/'));
        let last_updated_since = last_updated_since.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let authorization = format!("Bearer {token}");
        let bytes = self.http.request_bytes(
            "lastUpdatedDate request",
            NMS_JSON_RESPONSE_LIMIT,
            |agent| {
                agent
                    .get(endpoint.as_str())
                    .query("lastUpdatedDate", last_updated_since.as_str())
                    .query("classification", classification.api_name())
                    .header("Authorization", authorization.as_str())
                    .header("nmsResponseFormat", "AIXM")
                    .config()
                    .http_status_as_error(false)
                    .max_redirects(0)
                    .build()
                    .call()
            },
        )?;
        parse_notam_update_response(&bytes)
    }
}

fn parse_notam_update_response(bytes: &[u8]) -> anyhow::Result<Vec<String>> {
    let response = serde_json::from_slice::<NotamUpdateResponse>(bytes).with_context(|| {
        format!(
            "NMS lastUpdatedDate response was not valid JSON (bytes={}, sha256={:x})",
            bytes.len(),
            Sha256::digest(bytes)
        )
    })?;
    if !response.errors.is_empty() {
        let first_error = serde_json::to_string(&response.errors[0])
            .unwrap_or_else(|_| "<unprintable error>".to_string());
        bail!(
            "NMS lastUpdatedDate response contained {} errors; first error: {}",
            response.errors.len(),
            first_error.chars().take(500).collect::<String>()
        );
    }
    if response
        .status
        .as_deref()
        .is_some_and(|status| status != "Success")
    {
        bail!(
            "NMS lastUpdatedDate request returned status {}",
            response.status.as_deref().unwrap_or_default()
        );
    }
    response
        .data
        .context("NMS lastUpdatedDate response contained no data")
        .map(|data| data.aixm)
}

impl InitialLoadSource for NmsClient {
    fn capture_source(&self) -> InitialLoadCaptureSource {
        InitialLoadCaptureSource {
            environment: self.config.source_environment.clone(),
            api_base_url: Some(self.config.api_base_url.clone()),
        }
    }

    fn fetch_classification(
        &mut self,
        classification: NmsNotamClassification,
        output_gzip_path: &Path,
    ) -> anyhow::Result<()> {
        let token = self.access_token()?;
        let content = self.request_content_url(&token, classification)?;
        self.download_content(&token, &content, output_gzip_path)
    }
}

impl NmsApiSource for NmsClient {
    fn fetch_updates(
        &mut self,
        classification: NmsNotamClassification,
        last_updated_since: DateTime<Utc>,
    ) -> anyhow::Result<Vec<String>> {
        let token = self.access_token()?;
        self.request_updates(&token, classification, last_updated_since)
    }
}

fn default_token_lifetime_seconds() -> u64 {
    30 * 60
}

fn deserialize_u64_or_string<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Value {
        Number(u64),
        String(String),
    }

    match Value::deserialize(deserializer)? {
        Value::Number(value) => Ok(value),
        Value::String(value) => value.parse().map_err(serde::de::Error::custom),
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct CaptureManifest {
    schema_version: u32,
    captured_at_utc: String,
    source: InitialLoadCaptureSource,
    classifications: Vec<ClassificationCaptureManifest>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ClassificationCaptureManifest {
    classification: NmsNotamClassification,
    feature_collection_timestamp: Option<String>,
    declared_record_count: Option<usize>,
    parsed_message_count: usize,
    canonical_record_count: usize,
    gzip: CapturedFile,
    xml: CapturedFile,
    normalized_records: CapturedFile,
}

#[derive(Debug, Serialize, Deserialize)]
struct CapturedFile {
    path: String,
    bytes: u64,
    sha256: String,
}

pub struct LoadedInitialLoadCapture {
    pub source: InitialLoadCaptureSource,
    pub captured_at_utc: DateTime<Utc>,
    pub records: Vec<StructuredNotamRecord>,
}

pub fn capture_initial_load(
    output_dir: &Path,
    classifications: &[NmsNotamClassification],
    source: &mut impl InitialLoadSource,
) -> anyhow::Result<()> {
    if classifications.is_empty() {
        bail!("at least one NMS Initial Load classification is required");
    }
    if output_dir.exists() {
        bail!("capture output already exists: {}", output_dir.display());
    }
    let parent = output_dir
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("failed to create capture parent {}", parent.display()))?;
    let output_name = output_dir
        .file_name()
        .context("capture output must have a final path component")?
        .to_string_lossy();
    let temp_dir = parent.join(format!(
        ".{output_name}.tmp-{}-{}",
        std::process::id(),
        Utc::now().timestamp_millis()
    ));
    fs::create_dir(&temp_dir)
        .with_context(|| format!("failed to create temporary capture {}", temp_dir.display()))?;

    let result = build_capture(&temp_dir, classifications, source).and_then(|manifest| {
        let manifest_path = temp_dir.join("manifest.json");
        let file = File::create(&manifest_path)
            .with_context(|| format!("failed to create {}", manifest_path.display()))?;
        serde_json::to_writer_pretty(BufWriter::new(file), &manifest)
            .with_context(|| format!("failed to write {}", manifest_path.display()))?;
        fs::rename(&temp_dir, output_dir).with_context(|| {
            format!(
                "failed to publish NMS Initial Load capture {}",
                output_dir.display()
            )
        })
    });
    result.with_context(|| {
        format!(
            "incomplete NMS Initial Load capture retained for diagnosis at {}",
            temp_dir.display()
        )
    })
}

fn build_capture(
    temp_dir: &Path,
    classifications: &[NmsNotamClassification],
    source: &mut impl InitialLoadSource,
) -> anyhow::Result<CaptureManifest> {
    let captured_at_utc = Utc::now().to_rfc3339();
    let capture_source = source.capture_source();
    let mut manifests = Vec::new();
    for &classification in classifications {
        let stem = classification.api_name().to_ascii_lowercase();
        let gzip_name = format!("{stem}.xml.gz");
        let xml_name = format!("{stem}.xml");
        let normalized_name = format!("{stem}.normalized.json");
        let gzip_path = temp_dir.join(&gzip_name);
        let xml_path = temp_dir.join(&xml_name);
        let normalized_path = temp_dir.join(&normalized_name);

        source
            .fetch_classification(classification, &gzip_path)
            .with_context(|| {
                format!("failed to fetch {} Initial Load", classification.api_name())
            })?;
        decompress_gzip(&gzip_path, &xml_path)?;
        let xml_file = File::open(&xml_path)
            .with_context(|| format!("failed to open {}", xml_path.display()))?;
        let parsed = parse_nms_initial_load(BufReader::new(xml_file), classification)
            .with_context(|| {
                format!("failed to parse {} Initial Load", classification.api_name())
            })?;
        write_normalized_records(&normalized_path, &parsed)?;
        write_parse_diagnostics(&temp_dir.join(format!("{stem}.parse.json")), &parsed)?;
        parsed.validate_complete()?;

        manifests.push(ClassificationCaptureManifest {
            classification,
            feature_collection_timestamp: parsed.feature_collection_timestamp,
            declared_record_count: parsed.declared_record_count,
            parsed_message_count: parsed.parsed_message_count,
            canonical_record_count: parsed.records.len(),
            gzip: captured_file(&gzip_path, gzip_name)?,
            xml: captured_file(&xml_path, xml_name)?,
            normalized_records: captured_file(&normalized_path, normalized_name)?,
        });
    }
    Ok(CaptureManifest {
        schema_version: 1,
        captured_at_utc,
        source: capture_source,
        classifications: manifests,
    })
}

fn write_normalized_records(path: &Path, parsed: &NmsInitialLoadParseResult) -> anyhow::Result<()> {
    let file =
        File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    serde_json::to_writer(BufWriter::new(file), &parsed.records)
        .with_context(|| format!("failed to write {}", path.display()))
}

fn write_parse_diagnostics(path: &Path, parsed: &NmsInitialLoadParseResult) -> anyhow::Result<()> {
    let file =
        File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    serde_json::to_writer_pretty(
        BufWriter::new(file),
        &serde_json::json!({
            "classification": parsed.classification,
            "feature_collection_timestamp": parsed.feature_collection_timestamp,
            "declared_record_count": parsed.declared_record_count,
            "parsed_message_count": parsed.parsed_message_count,
            "canonical_record_count": parsed.records.len(),
            "duplicate_record_ids": parsed.duplicate_record_ids,
            "rejections": parsed.rejections,
        }),
    )
    .with_context(|| format!("failed to write {}", path.display()))
}

fn decompress_gzip(input_path: &Path, output_path: &Path) -> anyhow::Result<()> {
    let input = File::open(input_path)
        .with_context(|| format!("failed to open {}", input_path.display()))?;
    let output = File::create(output_path)
        .with_context(|| format!("failed to create {}", output_path.display()))?;
    let mut decoder = GzDecoder::new(BufReader::new(input));
    let mut writer = BufWriter::new(output);
    std::io::copy(&mut decoder, &mut writer)
        .with_context(|| format!("failed to decompress {}", input_path.display()))?;
    writer
        .flush()
        .with_context(|| format!("failed to flush {}", output_path.display()))
}

fn captured_file(path: &Path, relative_path: String) -> anyhow::Result<CapturedFile> {
    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut digest = Sha256::new();
    let bytes = std::io::copy(&mut file, &mut digest)
        .with_context(|| format!("failed to hash {}", path.display()))?;
    Ok(CapturedFile {
        path: relative_path,
        bytes,
        sha256: format!("{:x}", digest.finalize()),
    })
}

#[derive(Debug)]
struct ResolvedContentUrl {
    url: String,
    send_bearer: bool,
}

fn resolve_content_url(
    api_base_url: &str,
    content_url: &str,
) -> anyhow::Result<ResolvedContentUrl> {
    validate_no_control(content_url, "NMS content URL")?;
    let api_base_url = api_base_url.trim_end_matches('/');
    let api_origin = https_origin(api_base_url)?;
    let service_base = api_base_url
        .strip_suffix("/v1")
        .context("NMS apiBaseUrl must end in /v1")?;

    if content_url.contains("://") && !content_url.starts_with("https://") {
        bail!("NMS content URL must use HTTPS");
    }

    let url = if content_url.starts_with("https://") {
        content_url.to_string()
    } else if content_url.starts_with("/v1/") {
        format!("{service_base}{content_url}")
    } else if content_url.starts_with('/') {
        format!("{api_origin}{content_url}")
    } else {
        format!("{api_base_url}/{content_url}")
    };
    let content_origin = https_origin(&url)?;
    Ok(ResolvedContentUrl {
        send_bearer: content_origin == api_origin,
        url,
    })
}

fn https_origin(url: &str) -> anyhow::Result<&str> {
    let remainder = url
        .strip_prefix("https://")
        .context("NMS URL must use HTTPS")?;
    let authority_len = remainder.find('/').unwrap_or(remainder.len());
    let origin_len = "https://".len() + authority_len;
    Ok(&url[..origin_len])
}

fn validate_https_url(value: &str, label: &str) -> anyhow::Result<()> {
    validate_no_control(value, label)?;
    https_origin(value).map(|_| ())
}

fn validate_secret_value(value: &str, label: &str) -> anyhow::Result<()> {
    if value.is_empty() {
        bail!("NMS {label} cannot be empty");
    }
    validate_no_control(value, label)
}

fn validate_no_control(value: &str, label: &str) -> anyhow::Result<()> {
    if value.chars().any(char::is_control) {
        bail!("{label} cannot contain control characters");
    }
    Ok(())
}

struct NmsHttpClient {
    agent: ureq::Agent,
    attempts: usize,
    retry_delay: Duration,
}

impl NmsHttpClient {
    fn new(attempts: usize, retry_delay: Duration) -> Self {
        assert!(attempts > 0, "NMS HTTP attempts must be positive");
        Self {
            agent: ureq::Agent::new_with_defaults(),
            attempts,
            retry_delay,
        }
    }

    fn request_bytes(
        &self,
        operation: &str,
        response_limit: u64,
        send: impl FnMut(&ureq::Agent) -> Result<ureq::http::Response<ureq::Body>, ureq::Error>,
    ) -> anyhow::Result<Vec<u8>> {
        self.request(operation, send, |response| {
            response
                .body_mut()
                .with_config()
                .limit(response_limit)
                .read_to_vec()
                .with_context(|| format!("failed to read NMS {operation} response"))
        })
    }

    fn request<T>(
        &self,
        operation: &str,
        mut send: impl FnMut(&ureq::Agent) -> Result<ureq::http::Response<ureq::Body>, ureq::Error>,
        mut consume: impl FnMut(&mut ureq::http::Response<ureq::Body>) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        for attempt in 1..=self.attempts {
            let (error, retryable) = match send(&self.agent) {
                Ok(mut response) if response.status().is_success() => {
                    match consume(&mut response) {
                        Ok(value) => return Ok(value),
                        Err(error) => (
                            error.context(format!(
                                "failed to consume NMS {operation} response on attempt {attempt}"
                            )),
                            true,
                        ),
                    }
                }
                Ok(mut response) => {
                    let status = response.status();
                    let retryable = is_retryable_http_status(status.as_u16());
                    let detail = response_error_preview(&mut response);
                    (
                        anyhow::anyhow!(
                            "NMS {operation} returned HTTP {} on attempt {attempt}: {detail}",
                            status.as_u16()
                        ),
                        retryable,
                    )
                }
                Err(error) => (
                    anyhow::anyhow!(
                        "NMS {operation} transport failed on attempt {attempt}: {error}"
                    ),
                    true,
                ),
            };
            if !retryable || attempt == self.attempts {
                return Err(error);
            }
            if !self.retry_delay.is_zero() {
                std::thread::sleep(self.retry_delay);
            }
        }
        unreachable!("positive NMS HTTP attempt count exhausted without a result")
    }
}

fn is_retryable_http_status(status: u16) -> bool {
    status == 408 || status == 429 || status >= 500
}

fn response_error_preview(response: &mut ureq::http::Response<ureq::Body>) -> String {
    let mut bytes = Vec::new();
    match response
        .body_mut()
        .as_reader()
        .take(NMS_HTTP_ERROR_PREVIEW_LIMIT)
        .read_to_end(&mut bytes)
    {
        Ok(_) if bytes.is_empty() => "<empty response body>".to_string(),
        Ok(_) => String::from_utf8_lossy(&bytes)
            .chars()
            .map(|character| {
                if character.is_control() && !character.is_ascii_whitespace() {
                    '\u{fffd}'
                } else {
                    character
                }
            })
            .collect(),
        Err(error) => format!("<failed to read response body: {error}>"),
    }
}

pub fn load_initial_load_capture(capture_dir: &Path) -> anyhow::Result<LoadedInitialLoadCapture> {
    let manifest_path = capture_dir.join("manifest.json");
    let manifest = serde_json::from_reader::<_, CaptureManifest>(BufReader::new(
        File::open(&manifest_path)
            .with_context(|| format!("failed to open {}", manifest_path.display()))?,
    ))
    .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    if manifest.schema_version != 1 {
        bail!(
            "unsupported NMS Initial Load capture schema {}",
            manifest.schema_version
        );
    }
    let captured_at_utc = DateTime::parse_from_rfc3339(&manifest.captured_at_utc)
        .context("NMS Initial Load capture has invalid captured_at_utc")?
        .with_timezone(&Utc);
    let mut records = Vec::new();
    for classification in &manifest.classifications {
        let path = capture_dir.join(&classification.normalized_records.path);
        let mut classification_records =
            serde_json::from_reader::<_, Vec<StructuredNotamRecord>>(BufReader::new(
                File::open(&path).with_context(|| format!("failed to open {}", path.display()))?,
            ))
            .with_context(|| format!("failed to parse {}", path.display()))?;
        records.append(&mut classification_records);
    }
    Ok(LoadedInitialLoadCapture {
        source: manifest.source,
        captured_at_utc,
        records,
    })
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read};
    use std::net::TcpListener;
    use std::thread;

    use flate2::write::GzEncoder;
    use flate2::Compression;
    use tempfile::tempdir;

    use super::*;

    const FIXTURE: &str =
        include_str!("../../preprocessor-live-feeds/tests/fixtures/nms_initial_load.xml");

    struct FixtureSource {
        xml: String,
    }

    impl InitialLoadSource for FixtureSource {
        fn capture_source(&self) -> InitialLoadCaptureSource {
            InitialLoadCaptureSource {
                environment: "fixture".to_string(),
                api_base_url: None,
            }
        }

        fn fetch_classification(
            &mut self,
            _classification: NmsNotamClassification,
            output_gzip_path: &Path,
        ) -> anyhow::Result<()> {
            let output = File::create(output_gzip_path)?;
            let mut encoder = GzEncoder::new(output, Compression::default());
            std::io::copy(&mut Cursor::new(self.xml.as_bytes()), &mut encoder)?;
            encoder.finish()?;
            Ok(())
        }
    }

    #[test]
    fn publishes_a_validated_capture_atomically() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let output = temp.path().join("capture");
        capture_initial_load(
            &output,
            &[NmsNotamClassification::Domestic],
            &mut FixtureSource {
                xml: FIXTURE.to_string(),
            },
        )?;

        assert!(output.join("manifest.json").is_file());
        assert!(output.join("domestic.xml.gz").is_file());
        assert!(output.join("domestic.xml").is_file());
        assert!(output.join("domestic.normalized.json").is_file());
        let manifest =
            serde_json::from_slice::<CaptureManifest>(&fs::read(output.join("manifest.json"))?)?;
        assert_eq!(manifest.source.environment, "fixture");
        assert_eq!(manifest.source.api_base_url, None);
        Ok(())
    }

    #[test]
    fn incomplete_capture_is_not_published() -> anyhow::Result<()> {
        let temp = tempdir()?;
        let output = temp.path().join("capture");
        let result = capture_initial_load(
            &output,
            &[NmsNotamClassification::Domestic],
            &mut FixtureSource {
                xml: FIXTURE.replacen("numberReturned=\"1\"", "numberReturned=\"2\"", 1),
            },
        );

        assert!(result.is_err());
        assert!(!output.exists());
        Ok(())
    }

    #[test]
    fn external_content_url_does_not_receive_bearer() -> anyhow::Result<()> {
        let resolved = resolve_content_url(
            "https://api-staging.cgifederal-aim.com/nmsapi/v1",
            "https://storage.googleapis.com/example/signed?secret=value",
        )?;
        assert!(!resolved.send_bearer);
        Ok(())
    }

    #[test]
    fn relative_content_url_receives_bearer() -> anyhow::Result<()> {
        let resolved = resolve_content_url(
            "https://api-staging.cgifederal-aim.com/nmsapi/v1",
            "/v1/content/initial-load.gz",
        )?;
        assert_eq!(
            resolved.url,
            "https://api-staging.cgifederal-aim.com/nmsapi/v1/content/initial-load.gz"
        );
        assert!(resolved.send_bearer);
        Ok(())
    }

    #[test]
    fn update_response_accepts_valid_data_without_advisory_status() -> anyhow::Result<()> {
        let updates = parse_notam_update_response(
            br#"{"errors":[],"data":{"aixm":["<AIXMBasicMessage/>"]}}"#,
        )?;
        assert_eq!(updates, vec!["<AIXMBasicMessage/>"]);
        Ok(())
    }

    #[test]
    fn update_response_rejects_reported_errors() {
        let error = parse_notam_update_response(
            br#"{"errors":[{"message":"bad query"}],"data":{"aixm":[]}}"#,
        )
        .expect_err("FAA error response was accepted");
        assert!(format!("{error:#}").contains("bad query"));
    }

    #[test]
    fn native_http_retry_discards_failed_http_response_body() -> anyhow::Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let server = thread::spawn(move || -> anyhow::Result<()> {
            for (status, body) in [
                ("503 Service Unavailable", r#"{"error":"temporary"}"#),
                (
                    "200 OK",
                    r#"{"errors":[],"data":{"aixm":["successful retry"]}}"#,
                ),
            ] {
                let (mut stream, _) = listener.accept()?;
                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request)?;
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )?;
            }
            Ok(())
        });
        let client = NmsHttpClient::new(2, Duration::ZERO);
        let response = client.request_bytes("retry test", NMS_JSON_RESPONSE_LIMIT, |agent| {
            agent
                .get(format!("http://{address}/updates"))
                .config()
                .http_status_as_error(false)
                .build()
                .call()
        })?;
        server
            .join()
            .map_err(|_| anyhow::anyhow!("test HTTP server panicked"))??;

        assert_eq!(
            response,
            br#"{"errors":[],"data":{"aixm":["successful retry"]}}"#
        );
        assert_eq!(
            parse_notam_update_response(&response)?,
            vec!["successful retry"]
        );
        Ok(())
    }

    #[test]
    fn native_http_does_not_retry_authentication_failure() -> anyhow::Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let address = listener.local_addr()?;
        let server = thread::spawn(move || -> anyhow::Result<()> {
            let (mut stream, _) = listener.accept()?;
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request)?;
            let body = r#"{"error":"bad token"}"#;
            write!(
                stream,
                "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )?;
            Ok(())
        });
        let client = NmsHttpClient::new(5, Duration::ZERO);
        let error = client
            .request_bytes("authentication test", NMS_JSON_RESPONSE_LIMIT, |agent| {
                agent
                    .get(format!("http://{address}/updates"))
                    .config()
                    .http_status_as_error(false)
                    .build()
                    .call()
            })
            .expect_err("HTTP 401 was accepted");
        server
            .join()
            .map_err(|_| anyhow::anyhow!("test HTTP server panicked"))??;

        let detail = format!("{error:#}");
        assert!(detail.contains("HTTP 401 on attempt 1"), "{detail}");
        assert!(detail.contains("bad token"), "{detail}");
        Ok(())
    }
}
