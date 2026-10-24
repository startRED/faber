//! Offline document validation only. No signature, storage, network or installation authority.

use chrono::{DateTime, Timelike, Utc};
use serde::de::{Error, IgnoredAny, MapAccess, SeqAccess, Visitor, value::MapAccessDeserializer};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;
use std::marker::PhantomData;

const MAX_REVISION: u64 = 9_007_199_254_740_991;
const MAX_BYTES: usize = 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum Invalid {
    Size,
    Schema,
    Freshness,
    Support,
    Revision,
}

#[derive(Debug, Clone)]
pub struct Accepted {
    pub revision: u64,
    pub digest: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub schema: u8,
    pub revision: u64,
    pub issued_at: String,
    pub expires_at: String,
    pub minimum_app_version: String,
    #[serde(deserialize_with = "collection::<_, _, 512>")]
    pub catalog: Vec<CatalogPin>,
    #[serde(deserialize_with = "collection::<_, _, 512>")]
    pub compatibility: Vec<Compatibility>,
    #[serde(deserialize_with = "provider_policies")]
    pub policies: Vec<Policy>,
    #[serde(deserialize_with = "collection::<_, _, 64>")]
    pub drivers: Vec<Driver>,
    #[serde(skip)]
    pub digest: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogPin {
    pub id: String,
    pub commit: String,
    pub digest: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Compatibility {
    pub provider: String,
    pub version: String,
    pub operating_system: String,
    pub architecture: String,
    #[serde(deserialize_with = "required_nullable")]
    pub artifact_digest: Option<String>,
    pub driver: String,
    pub outcome: String,
    #[serde(deserialize_with = "named_object")]
    pub evidence: Evidence,
    pub reason: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    #[serde(deserialize_with = "required_nullable")]
    pub startup: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    pub turn: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    pub cancellation: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    pub cleanup: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub provider: String,
    pub install: String,
    pub launch: String,
    pub reason: String,
    pub evidence: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Driver {
    pub id: String,
    pub provider: String,
    pub variant: String,
}

struct Object<T>(T);

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Object<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        named_object(deserializer).map(Self)
    }
}

fn named_object<'de, D: Deserializer<'de>, T: Deserialize<'de>>(deserializer: D) -> Result<T, D::Error> {
    struct Named<T>(PhantomData<T>);

    impl<'de, T: Deserialize<'de>> Visitor<'de> for Named<T> {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("an object with named fields")
        }

        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_map(Named(PhantomData))
}

fn collection<'de, D: Deserializer<'de>, T: Deserialize<'de>, const LIMIT: usize>(deserializer: D) -> Result<Vec<T>, D::Error> {
    struct Limited<T, const LIMIT: usize>(PhantomData<T>);

    impl<'de, T: Deserialize<'de>, const LIMIT: usize> Visitor<'de> for Limited<T, LIMIT> {
        type Value = Vec<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(formatter, "at most {LIMIT} named objects")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Vec<T>, A::Error> {
            let mut entries = Vec::new();
            while entries.len() < LIMIT {
                let Some(Object(entry)) = sequence.next_element::<Object<T>>()? else {
                    return Ok(entries);
                };
                entries.push(entry);
            }
            if sequence.next_element::<IgnoredAny>()?.is_some() {
                return Err(A::Error::custom("manifest collection exceeds limit"));
            }
            Ok(entries)
        }
    }

    deserializer.deserialize_seq(Limited::<T, LIMIT>(PhantomData))
}

fn provider_policies<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Policy>, D::Error> {
    collection::<D, Policy, { crate::model::Vendor::ALL.len() }>(deserializer)
}

// Unlike default Option deserialization, a nullable security field must still be present.
fn required_nullable<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}

fn hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.bytes().all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn reason(value: &str) -> bool {
    !value.is_empty() && value.chars().count() <= 512 && !value.chars().any(char::is_control)
}

fn provider(value: &str) -> bool {
    crate::model::Vendor::ALL.iter().any(|vendor| vendor.name() == value)
}

fn date(value: &str) -> Result<DateTime<Utc>, Invalid> {
    let parsed = DateTime::parse_from_rfc3339(value).map_err(|_| Invalid::Schema)?;
    if value.len() != 20 || parsed.nanosecond() >= 1_000_000_000 || parsed.format("%Y-%m-%dT%H:%M:%SZ").to_string() != value {
        return Err(Invalid::Schema);
    }
    Ok(parsed.with_timezone(&Utc))
}

struct Version {
    number: [u32; 3],
    prerelease: bool,
    build: bool,
}

fn identifiers(value: &str, prerelease: bool) -> bool {
    value.split('.').all(|part| {
        !part.is_empty()
            && part.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && !(prerelease && part.len() > 1 && part.starts_with('0') && part.bytes().all(|byte| byte.is_ascii_digit()))
    })
}

fn version(value: &str) -> Result<Version, Invalid> {
    if value.len() > 128 || !value.is_ascii() {
        return Err(Invalid::Schema);
    }
    let (value, build) = match value.split_once('+') {
        Some((value, build)) if identifiers(build, false) => (value, true),
        Some(_) => return Err(Invalid::Schema),
        None => (value, false),
    };
    let (core, prerelease) = match value.split_once('-') {
        Some((core, prerelease)) if identifiers(prerelease, true) => (core, true),
        Some(_) => return Err(Invalid::Schema),
        None => (value, false),
    };
    let component = |value: Option<&str>| {
        let value = value.ok_or(Invalid::Schema)?;
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) || value.len() > 1 && value.starts_with('0') {
            return Err(Invalid::Schema);
        }
        value.parse::<u32>().map_err(|_| Invalid::Schema)
    };
    let mut components = core.split('.');
    let number = [component(components.next())?, component(components.next())?, component(components.next())?];
    if components.next().is_some() {
        return Err(Invalid::Schema);
    }
    Ok(Version { number, prerelease, build })
}

/// Analyze authenticated bytes, never authorize installation. `previous: None` supplies no revision floor for
/// offline analysis; it does not prove first use or permit recovery from missing/damaged durable trust state.
/// The production caller must independently authenticate bytes and safely persist matching state and cache.
pub fn validate_document(payload: &[u8], now: &str, app_version: &str, previous: Option<&Accepted>) -> Result<Document, Invalid> {
    if payload.len() > MAX_BYTES {
        return Err(Invalid::Size);
    }
    // Typed, closed objects reject duplicates as well as unknown fields; no recursive remote object is accepted.
    let mut document = serde_json::from_slice::<Object<Document>>(payload).map_err(|_| Invalid::Schema)?.0;
    if document.schema != 1
        || !(1..=MAX_REVISION).contains(&document.revision)
        || document.policies.len() != crate::model::Vendor::ALL.len()
    {
        return Err(Invalid::Schema);
    }
    let issued = date(&document.issued_at)?;
    let expires = date(&document.expires_at)?;
    let now = date(now)?;
    if issued > now || expires <= now || expires <= issued {
        return Err(Invalid::Freshness);
    }
    let minimum = version(&document.minimum_app_version)?;
    if minimum.prerelease || minimum.build {
        return Err(Invalid::Schema);
    }
    let current = version(app_version)?;
    if current.number < minimum.number || current.number == minimum.number && current.prerelease {
        return Err(Invalid::Support);
    }
    let mut catalog_ids = BTreeSet::new();
    for pin in &document.catalog {
        if !catalog_ids.insert(&pin.id)
            || !crate::tools::catalog().iter().any(|item| item.id == pin.id && item.commit == pin.commit && item.digest == pin.digest)
        {
            return Err(Invalid::Support);
        }
    }
    let mut policies = BTreeSet::new();
    for policy in &document.policies {
        if !provider(&policy.provider)
            || !policies.insert(&policy.provider)
            || !matches!(policy.install.as_str(), "allow" | "suspend")
            || !matches!(policy.launch.as_str(), "allow" | "suspend")
            || !reason(&policy.reason)
            || !hash(&policy.evidence)
        {
            return Err(Invalid::Schema);
        }
    }
    let mut drivers = BTreeSet::new();
    for driver in &document.drivers {
        if !id(&driver.id)
            || !drivers.insert(&driver.id)
            || !matches!((driver.provider.as_str(), driver.variant.as_str()), ("claude", "stream-json") | ("codex", "app-server"))
        {
            return Err(Invalid::Support);
        }
    }
    let mut records = BTreeSet::new();
    let mut used_drivers = BTreeSet::new();
    for record in &document.compatibility {
        version(&record.version)?;
        let qualified = match record.outcome.as_str() {
            "qualified" => true,
            "unqualified" => false,
            _ => return Err(Invalid::Schema),
        };
        let evidence = [
            &record.evidence.startup,
            &record.evidence.turn,
            &record.evidence.cancellation,
            &record.evidence.cleanup,
            &record.artifact_digest,
        ];
        if !provider(&record.provider)
            || !matches!(
                (record.operating_system.as_str(), record.architecture.as_str()),
                ("windows", "x86_64") | ("macos" | "linux", "x86_64" | "aarch64")
            )
            || !records.insert((&record.provider, &record.version, &record.operating_system, &record.architecture))
            || !document.drivers.iter().any(|driver| driver.id == record.driver && driver.provider == record.provider)
            || !reason(&record.reason)
            || evidence.iter().any(|value| value.as_ref().is_some_and(|value| !hash(value)) || qualified && value.is_none())
        {
            return Err(Invalid::Schema);
        }
        used_drivers.insert(&record.driver);
    }
    if used_drivers != drivers {
        return Err(Invalid::Schema);
    }
    document.digest = crate::hex(&Sha256::digest(payload));
    if let Some(previous) = previous {
        if !(1..=MAX_REVISION).contains(&previous.revision)
            || !hash(&previous.digest)
            || document.revision < previous.revision
            || document.revision == previous.revision && document.digest != previous.digest
        {
            return Err(Invalid::Revision);
        }
    }
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    thread_local! {
        static CONSTRUCTED: Cell<usize> = const { Cell::new(0) };
    }

    struct Entry;

    impl<'de> Deserialize<'de> for Entry {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            CONSTRUCTED.set(CONSTRUCTED.get() + 1);
            serde::de::IgnoredAny::deserialize(deserializer)?;
            Ok(Self)
        }
    }

    #[test]
    fn live_manifest_collections_stop_before_constructing_an_excess_entry() {
        CONSTRUCTED.set(0);
        let mut deserializer = serde_json::Deserializer::from_str("[{},{},{}]");
        let result = collection::<_, Entry, 2>(&mut deserializer);
        assert!(result.is_err());
        assert_eq!(CONSTRUCTED.get(), 2, "an excess entry must not be constructed before refusing the list");
    }
}
