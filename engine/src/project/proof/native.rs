//! Native supply-chain checks. Public responses are bounded data, never instructions.
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Ecosystem {
    Cargo,
    Npm,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicPackage {
    pub ecosystem: Ecosystem,
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeError {
    Unknown,
    Source,
    TooBig,
    Malformed,
    License,
    Journal,
}
impl NativeError {
    pub fn key(self) -> &'static str {
        match self {
            Self::Unknown => "reason.proof.unknown",
            Self::Source => "error.proof.source",
            Self::TooBig => "error.proof.tooBig",
            Self::Malformed => "error.proof.malformed",
            Self::License => "error.proof.license",
            Self::Journal => "error.proof.receipt",
        }
    }
}

fn public_identity(package: &PublicPackage) -> Result<(), NativeError> {
    let name = &package.name;
    let good_name = match package.ecosystem {
        Ecosystem::Cargo => !name.is_empty() && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b)),
        Ecosystem::Npm => {
            let parts: Vec<_> = name.split('/').collect();
            let scoped = name.starts_with('@');
            parts.len() == if scoped { 2 } else { 1 }
                && parts.iter().enumerate().all(|(i, p)| {
                    let p = if scoped && i == 0 { p.strip_prefix('@').unwrap_or_default() } else { p };
                    !p.is_empty()
                        && p.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
                        && !p.starts_with('.')
                })
        }
    };
    if !good_name
        || name.len() > 214
        || package.version.is_empty()
        || package.version.len() > 128
        || !package.version.bytes().all(|b| b.is_ascii_alphanumeric() || b".-+".contains(&b))
    {
        return Err(NativeError::Malformed);
    }
    Ok(())
}

/// Only registry packages are sent to OSV. Unknown/private sources fail closed.
pub fn lock_packages(cargo: &str, npm: &str) -> Result<Vec<PublicPackage>, NativeError> {
    if cargo.len() > 8 * 1024 * 1024 || npm.len() > 8 * 1024 * 1024 {
        return Err(NativeError::TooBig);
    }
    let (header, _) = cargo.split_once("[[package]]").ok_or(NativeError::Unknown)?;
    let header = header.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')).collect::<Vec<_>>();
    if header.len() != 1 || !matches!(header[0], "version = 3" | "version = 4") {
        return Err(NativeError::Unknown);
    }
    let mut packages = BTreeSet::new();
    // Cargo emits these scalar fields as JSON-compatible quoted strings in lock v3/v4.
    for section in cargo.split("[[package]]").skip(1) {
        let mut name = None;
        let mut version = None;
        let mut source = None;
        for line in section.lines().take_while(|line| !line.trim().starts_with('[')) {
            let Some((key, value)) = line.split_once('=') else { continue };
            let slot = match key.trim() {
                "name" => &mut name,
                "version" => &mut version,
                "source" => &mut source,
                _ => continue,
            };
            if slot.is_some() {
                return Err(NativeError::Malformed);
            }
            *slot = Some(serde_json::from_str::<String>(value.trim()).map_err(|_| NativeError::Malformed)?);
        }
        let package = PublicPackage {
            ecosystem: Ecosystem::Cargo,
            name: name.ok_or(NativeError::Malformed)?,
            version: version.ok_or(NativeError::Malformed)?,
        };
        public_identity(&package)?;
        if let Some(source) = source {
            if !matches!(source.as_str(), "registry+https://github.com/rust-lang/crates.io-index" | "sparse+https://index.crates.io/") {
                return Err(NativeError::Source);
            }
            packages.insert(package);
        }
    }
    let npm: Value = serde_json::from_str(npm).map_err(|_| NativeError::Malformed)?;
    if !matches!(npm["lockfileVersion"].as_u64(), Some(2 | 3)) {
        return Err(NativeError::Unknown);
    }
    let nodes = npm["packages"].as_object().ok_or(NativeError::Malformed)?;
    for (path, record) in nodes {
        if path.is_empty() {
            continue;
        }
        let (_, inferred) = path.rsplit_once("node_modules/").ok_or(NativeError::Unknown)?;
        let package = PublicPackage {
            ecosystem: Ecosystem::Npm,
            name: record["name"].as_str().unwrap_or(inferred).into(),
            version: record["version"].as_str().ok_or(NativeError::Unknown)?.into(),
        };
        public_identity(&package)?;
        let resolved = record["resolved"].as_str().ok_or(NativeError::Unknown)?;
        let url = reqwest::Url::parse(resolved).map_err(|_| NativeError::Source)?;
        if url.host_str() != Some("registry.npmjs.org")
            || url.scheme() != "https"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port_or_known_default() != Some(443)
        {
            return Err(NativeError::Source);
        }
        packages.insert(package);
    }
    if packages.is_empty() {
        return Err(NativeError::Unknown);
    }
    Ok(packages.into_iter().collect())
}

pub fn osv_query(package: &PublicPackage) -> Result<Value, NativeError> {
    public_identity(package)?;
    let ecosystem = match package.ecosystem {
        Ecosystem::Cargo => "crates.io",
        Ecosystem::Npm => "npm",
    };
    Ok(json!({"package":{"ecosystem":ecosystem,"name":package.name},"version":package.version}))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdvisoryStatus {
    Clear,
    Vulnerable(Vec<String>),
}

pub fn parse_osv_response(package: &PublicPackage, body: &[u8]) -> Result<AdvisoryStatus, NativeError> {
    public_identity(package)?;
    if body.len() > 2 * 1024 * 1024 {
        return Err(NativeError::TooBig);
    }
    let value: Value = serde_json::from_slice(body).map_err(|_| NativeError::Malformed)?;
    let fields = value.as_object().ok_or(NativeError::Malformed)?;
    // OSV's empty query response is {}. An error or unrecognized shape is not a clear result.
    if fields.is_empty() {
        return Ok(AdvisoryStatus::Clear);
    }
    if fields.len() != 1 {
        return Err(NativeError::Unknown);
    }
    let vulns = value["vulns"].as_array().ok_or(NativeError::Unknown)?;
    let ids = vulns
        .iter()
        .map(|v| v["id"].as_str().filter(|id| !id.is_empty() && id.len() <= 128).map(String::from).ok_or(NativeError::Malformed))
        .collect::<Result<Vec<_>, _>>()?;
    if ids.is_empty() { Ok(AdvisoryStatus::Clear) } else { Ok(AdvisoryStatus::Vulnerable(ids)) }
}

/// Exact official hosts, HTTPS, no userinfo, custom port, query credentials or redirect.
pub fn validate_source_url(raw: &str) -> Result<reqwest::Url, NativeError> {
    let url = reqwest::Url::parse(raw).map_err(|_| NativeError::Source)?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port_or_known_default() != Some(443)
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(
            url.host_str(),
            Some(
                "api.osv.dev"
                    | "crates.io"
                    | "registry.npmjs.org"
                    | "api.npmjs.org"
                    | "www.w3.org"
                    | "doc.rust-lang.org"
                    | "tokio.rs"
                    | "tauri.app"
                    | "docs.solidjs.com"
                    | "osv.dev"
                    | "spdx.org"
                    | "docs.npmjs.com"
            )
        )
    {
        return Err(NativeError::Source);
    }
    Ok(url)
}

pub fn authorize_source(raw: &str, authorized: &BTreeSet<String>) -> Result<reqwest::Url, NativeError> {
    let url = validate_source_url(raw)?;
    if !authorized.contains(url.as_str()) {
        return Err(NativeError::Source);
    }
    Ok(url)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceReceipt {
    pub schema: u32,
    pub id: String,
    pub attempt: Option<String>,
    pub url: String,
    pub reason: String,
    pub fetched_at: u64,
    pub sha256: Option<String>,
    pub error: Option<String>,
    #[serde(flatten)]
    pub extra: super::super::Extra,
}
impl super::super::Record for SourceReceipt {
    fn schema(&self) -> u32 {
        self.schema
    }
}
impl SourceReceipt {
    pub fn validate(&self) -> Result<(), NativeError> {
        validate_source_url(&self.url)?;
        if self.schema != super::super::SCHEMA
            || !super::super::valid_id(&self.id)
            || self.attempt.as_ref().is_some_and(|id| !super::super::valid_id(id))
            || self.reason.is_empty()
            || self.reason.len() > 128
            || self.sha256.as_ref().is_some_and(|h| h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()))
        {
            return Err(NativeError::Malformed);
        }
        Ok(())
    }
}
pub struct PublicResponse {
    pub receipt: SourceReceipt,
    pub body: Option<Vec<u8>>,
}

async fn record_source(journal: &super::super::store::Journal, receipt: &SourceReceipt) -> Result<(), NativeError> {
    let journal = journal.clone();
    let receipt = receipt.clone();
    tokio::task::spawn_blocking(move || journal.record_source(&receipt))
        .await
        .map_err(|_| NativeError::Journal)?
        .map_err(|_| NativeError::Journal)
}

async fn fetch(
    url: &str,
    reason: &str,
    post: Option<Value>,
    authorized: &BTreeSet<String>,
    journal: &super::super::store::Journal,
) -> Result<PublicResponse, NativeError> {
    let mut receipt = SourceReceipt {
        schema: super::super::SCHEMA,
        id: uuid::Uuid::new_v4().to_string(),
        attempt: None,
        url: url.into(),
        reason: reason.into(),
        fetched_at: SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()),
        sha256: None,
        error: None,
        extra: Default::default(),
    };
    record_source(journal, &receipt).await?;
    receipt.attempt = Some(receipt.id.clone());
    receipt.id = uuid::Uuid::new_v4().to_string();
    let result = async {
        let url = authorize_source(url, authorized)?;
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| NativeError::Unknown)?;
        let request = if let Some(body) = post {
            client.post(url.clone()).header("Content-Type", "application/json").body(body.to_string())
        } else {
            client.get(url.clone())
        };
        let response = request.header("User-Agent", "faber-native-proof/1").send().await.map_err(|_| NativeError::Unknown)?;
        if !response.status().is_success() {
            return Err(NativeError::Unknown);
        }
        crate::install::metadata_body(response, 2 * 1024 * 1024, url.as_str()).await.map_err(|_| NativeError::Unknown)
    }
    .await;
    let body = match result {
        Ok(bytes) => {
            receipt.sha256 = Some(super::super::lease::hash(&bytes));
            Some(bytes)
        }
        Err(error) => {
            receipt.error = Some(error.key().into());
            None
        }
    };
    record_source(journal, &receipt).await?;
    Ok(PublicResponse { receipt, body })
}

pub async fn query_osv(
    package: &PublicPackage,
    authorized: &BTreeSet<String>,
    journal: &super::super::store::Journal,
) -> Result<PublicResponse, NativeError> {
    let body = osv_query(package)?;
    // npm also hosts private packages: a lockfile host or syntactically valid name
    // is not public provenance. Verify anonymously at the source registry first.
    if authorize_source("https://api.osv.dev/v1/query", authorized).is_ok() {
        verify_dependency_registry(package, &[], authorized, journal).await?;
    }
    fetch("https://api.osv.dev/v1/query", "proof.osv-public-package-version", Some(body), authorized, journal).await
}

pub async fn fetch_documentation(
    url: &str,
    authorized: &BTreeSet<String>,
    journal: &super::super::store::Journal,
) -> Result<PublicResponse, NativeError> {
    validate_source_url(url)?;
    fetch(url, "plan.ground-official-documentation", None, authorized, journal).await
}

pub async fn query_registry(
    package: &PublicPackage,
    authorized: &BTreeSet<String>,
    journal: &super::super::store::Journal,
) -> Result<Vec<PublicResponse>, NativeError> {
    public_identity(package)?;
    let urls = match package.ecosystem {
        Ecosystem::Cargo => vec![format!("https://crates.io/api/v1/crates/{}", package.name)],
        Ecosystem::Npm => vec![
            format!("https://registry.npmjs.org/{}", package.name),
            format!("https://api.npmjs.org/downloads/point/last-week/{}", package.name),
        ],
    };
    let mut responses = Vec::new();
    for url in urls {
        responses.push(fetch(&url, "proof.registry-public-metadata", None, authorized, journal).await?);
    }
    Ok(responses)
}

pub async fn verify_dependency_registry(
    package: &PublicPackage,
    popular: &[&str],
    authorized: &BTreeSet<String>,
    journal: &super::super::store::Journal,
) -> Result<RegistryMetadata, NativeError> {
    public_identity(package)?;
    super::check_package_name(&package.name, popular).map_err(|_| NativeError::Source)?;
    let responses = query_registry(package, authorized, journal).await?;
    let metadata = responses.first().and_then(|r| r.body.as_ref()).ok_or(NativeError::Unknown)?;
    let metadata: Value = serde_json::from_slice(metadata).map_err(|_| NativeError::Malformed)?;
    let additional = if package.ecosystem == Ecosystem::Npm {
        serde_json::from_slice(responses.get(1).and_then(|r| r.body.as_ref()).ok_or(NativeError::Unknown)?)
            .map_err(|_| NativeError::Malformed)?
    } else {
        Value::Null
    };
    parse_registry(package, &metadata, &additional)
}

/// Query every public lockfile package. Missing permission, transport, response or
/// coverage is an error, never an empty advisory database interpreted as clean.
pub async fn scan_lockfiles(
    root: &std::path::Path,
    authorized: &BTreeSet<String>,
    journal: &super::super::store::Journal,
) -> Result<Vec<(PublicPackage, AdvisoryStatus)>, NativeError> {
    let root = root.to_path_buf();
    let locks = tokio::task::spawn_blocking(move || {
        let mut locks = Vec::new();
        for name in ["Cargo.lock", "package-lock.json"] {
            let file = super::super::files::ScopedFile::new(&root.join(name), false).map_err(|_| NativeError::Source)?;
            let body = file.read_limited(8 * 1024 * 1024).map_err(|_| NativeError::Unknown)?.ok_or(NativeError::Unknown)?.0;
            locks.push(String::from_utf8(body).map_err(|_| NativeError::Malformed)?);
        }
        Ok::<_, NativeError>(locks)
    })
    .await
    .map_err(|_| NativeError::Unknown)??;
    let packages = lock_packages(&locks[0], &locks[1])?;
    let mut results = Vec::new();
    for package in packages {
        let response = query_osv(&package, authorized, journal).await?;
        let body = response.body.ok_or(NativeError::Unknown)?;
        let result = parse_osv_response(&package, &body)?;
        results.push((package, result));
    }
    Ok(results)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryMetadata {
    pub publisher: String,
    pub created_at: String,
    pub published_at: String,
    pub downloads: u64,
}
fn nonempty(value: &Value) -> Result<String, NativeError> {
    value.as_str().filter(|s| !s.trim().is_empty() && s.len() <= 1024).map(String::from).ok_or(NativeError::Unknown)
}
pub fn parse_registry(package: &PublicPackage, metadata: &Value, additional: &Value) -> Result<RegistryMetadata, NativeError> {
    public_identity(package)?;
    match package.ecosystem {
        Ecosystem::Npm => {
            let release = &metadata["versions"][&package.version];
            if metadata["name"] != package.name
                || release["name"] != package.name
                || release["version"] != package.version
                || additional["package"] != package.name
            {
                return Err(NativeError::Unknown);
            }
            Ok(RegistryMetadata {
                publisher: nonempty(&release["_npmUser"]["name"])?,
                created_at: nonempty(&metadata["time"]["created"])?,
                published_at: nonempty(&metadata["time"][&package.version])?,
                downloads: additional["downloads"].as_u64().ok_or(NativeError::Unknown)?,
            })
        }
        Ecosystem::Cargo => {
            if metadata["crate"]["id"] != package.name {
                return Err(NativeError::Unknown);
            }
            let versions = metadata["versions"].as_array().ok_or(NativeError::Unknown)?;
            let release = versions.iter().find(|v| v["num"] == package.version).ok_or(NativeError::Unknown)?;
            Ok(RegistryMetadata {
                publisher: nonempty(&release["published_by"]["login"])?,
                created_at: nonempty(&metadata["crate"]["created_at"])?,
                published_at: nonempty(&release["created_at"])?,
                downloads: metadata["crate"]["downloads"].as_u64().ok_or(NativeError::Unknown)?,
            })
        }
    }
}

/// Bounded SPDX AND/OR expressions. Unknown IDs and exceptions need owner review.
pub fn license_allowed(expression: &str, _commercial: bool) -> bool {
    fn atom(tokens: &[&str], pos: &mut usize) -> Option<bool> {
        let token = *tokens.get(*pos)?;
        *pos += 1;
        if token == "(" {
            let value = or(tokens, pos)?;
            if *tokens.get(*pos)? != ")" {
                return None;
            }
            *pos += 1;
            return Some(value);
        }
        if matches!(token, ")" | "AND" | "OR" | "WITH") {
            return None;
        }
        // Project purpose alone proves no license compatibility. Unsupported terms need review.
        let allowed =
            matches!(token, "MIT" | "Apache-2.0" | "BSD-2-Clause" | "BSD-3-Clause" | "ISC" | "CC0-1.0" | "0BSD" | "Zlib" | "FSL-1.1-ALv2");
        Some(allowed)
    }
    fn and(tokens: &[&str], pos: &mut usize) -> Option<bool> {
        let mut value = atom(tokens, pos)?;
        while tokens.get(*pos) == Some(&"AND") {
            *pos += 1;
            let rhs = atom(tokens, pos)?;
            value &= rhs;
        }
        Some(value)
    }
    fn or(tokens: &[&str], pos: &mut usize) -> Option<bool> {
        let mut value = and(tokens, pos)?;
        while tokens.get(*pos) == Some(&"OR") {
            *pos += 1;
            let rhs = and(tokens, pos)?;
            value |= rhs;
        }
        Some(value)
    }
    if expression.len() > 1024 {
        return false;
    }
    let spaced = expression.replace('(', " ( ").replace(')', " ) ");
    let tokens = spaced.split_whitespace().collect::<Vec<_>>();
    if tokens.len() > 64 {
        return false;
    }
    let mut pos = 0;
    or(&tokens, &mut pos) == Some(true) && pos == tokens.len()
}

pub fn cargo_licenses(metadata: &Value, commercial: bool) -> Result<(), NativeError> {
    let packages = metadata["packages"].as_array().filter(|p| !p.is_empty()).ok_or(NativeError::Unknown)?;
    for package in packages {
        nonempty(&package["name"])?;
        let license = nonempty(&package["license"])?;
        if !license_allowed(&license, commercial) {
            return Err(NativeError::License);
        }
    }
    Ok(())
}

pub fn cargo_licenses_complete(locked: &[PublicPackage], metadata: &Value, commercial: bool) -> Result<(), NativeError> {
    let packages = metadata["packages"].as_array().ok_or(NativeError::Unknown)?;
    for package in locked.iter().filter(|p| p.ecosystem == Ecosystem::Cargo) {
        if !packages.iter().any(|p| p["name"] == package.name && p["version"] == package.version) {
            return Err(NativeError::Unknown);
        }
    }
    cargo_licenses(metadata, commercial)
}

pub fn npm_licenses(lock: &Value, commercial: bool) -> Result<(), NativeError> {
    let packages = lock["packages"].as_object().ok_or(NativeError::Unknown)?;
    for (path, package) in packages {
        if path.is_empty() {
            continue;
        }
        let license = nonempty(&package["license"])?;
        if !license_allowed(&license, commercial) {
            return Err(NativeError::License);
        }
    }
    Ok(())
}
