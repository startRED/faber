//! How a command fails. Almost always it is an error key (`error.*`), which crosses as a plain string; the appearance
//! commands add the field or the pairs the key is about, so for them it crosses as `{ "key", "detail" }`. The screen reads
//! both (`failureOf` in `ui/api.ts`).

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// An error the screen can show: a translation key and, sometimes, the field or the pairs it is about.
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    pub key: String,
    pub detail: Option<String>,
    pub data: Option<serde_json::Value>,
}

impl Failure {
    pub fn key(key: &str) -> Self {
        Failure { key: key.into(), detail: None, data: None }
    }

    pub(crate) fn with(key: &str, detail: impl Into<String>) -> Self {
        Failure { key: key.into(), detail: Some(detail.into()), data: None }
    }
}

/// Engine errors are plain keys.
impl From<String> for Failure {
    fn from(key: String) -> Self {
        Failure { key, detail: None, data: None }
    }
}

impl From<&str> for Failure {
    fn from(key: &str) -> Self {
        Failure::key(key)
    }
}

#[derive(Serialize, Deserialize)]
struct Detailed {
    key: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    data: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum Shape {
    Key(String),
    Detailed(Detailed),
}

impl Serialize for Failure {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match (&self.detail, &self.data) {
            (None, None) => Shape::Key(self.key.clone()),
            _ => Shape::Detailed(Detailed { key: self.key.clone(), detail: self.detail.clone(), data: self.data.clone() }),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Failure {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match Shape::deserialize(deserializer)? {
            Shape::Key(key) => Failure { key, detail: None, data: None },
            Shape::Detailed(Detailed { key, detail, data }) => Failure { key, detail, data },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_bare_key_crosses_as_a_string_and_a_detailed_one_as_an_object() {
        assert_eq!(serde_json::to_value(Failure::key("error.x")).unwrap(), json!("error.x"));
        assert_eq!(
            serde_json::to_value(Failure::with("error.look.pairFailed", "ink, bg")).unwrap(),
            json!({ "key": "error.look.pairFailed", "detail": "ink, bg" })
        );
        for failure in [Failure::key("error.x"), Failure::with("error.y", "field")] {
            assert_eq!(serde_json::from_value::<Failure>(serde_json::to_value(&failure).unwrap()).unwrap(), failure);
        }
    }
}
