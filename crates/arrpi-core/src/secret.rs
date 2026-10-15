//! Redacted credentials with zeroizing heap storage.

use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Deserializer, Serialize};

/// Shared client-key grammar used by configuration and protocol boundaries.
#[must_use]
pub fn valid_client_key(value: &str) -> bool {
    (32..=128).contains(&value.len()) && value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}

/// An optional credential that serializes as null and clears its heap allocation on drop.
#[derive(Clone, Default)]
pub struct Secret(Option<SecretString>);

impl Secret {
    /// Create an omitted credential for initial state or masked update preservation.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Expose a borrowed credential only for authorized configuration or adapter operations.
    #[must_use]
    pub fn value(&self) -> &str {
        self.0.as_ref().map_or("", ExposeSecret::expose_secret)
    }

    /// Whether an update omitted the credential and should retain the stored value.
    #[must_use]
    pub const fn missing(&self) -> bool {
        self.0.is_none()
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<String>::deserialize(deserializer).map(|value| Self(value.map(SecretString::from)))
    }
}

impl Serialize for Secret {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_none()
    }
}

impl From<String> for Secret {
    fn from(value: String) -> Self {
        Self(Some(value.into()))
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl schemars::JsonSchema for Secret {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Credential".into()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        <Option<String>>::json_schema(generator)
    }
}
