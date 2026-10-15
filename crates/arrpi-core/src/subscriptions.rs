//! Optional local subscription storage and rolling calendar limits.

use serde::{Deserialize, Serialize};

use crate::config::ConfigError;

/// Personal feed issuance and bounded rolling calendar windows.
#[derive(Clone, Debug, Deserialize, Serialize, schemars::JsonSchema)]
#[serde(default, deny_unknown_fields)]
#[schemars(rename = "SubscriptionPolicy")]
pub struct Policy {
    pub enabled: bool,
    pub master_key: crate::secret::Secret,
    #[schemars(range(min = 1, max = 4096))]
    pub max_subjects: usize,
    #[schemars(range(min = 0, max = 92))]
    pub past_days: u32,
    #[schemars(range(min = 0, max = 92))]
    pub future_days: u32,
}

impl Policy {
    /// Validate rolling feed windows and enabled subscription credentials.
    ///
    /// # Errors
    /// Returns a redacted error for excessive windows, capacity or a missing key.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if !(1..=4096).contains(&self.max_subjects)
            || self
                .past_days
                .checked_add(self.future_days)
                .is_none_or(|days| days > 92)
            || (self.enabled && !(32..=128).contains(&self.master_key.value().len()))
        {
            return Err(ConfigError::invalid("invalid subscription policy"));
        }

        Ok(())
    }
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            enabled: false,
            master_key: crate::secret::Secret::default(),
            max_subjects: 4096,
            past_days: 7,
            future_days: 60,
        }
    }
}
