//! The env slot: the server's environment variables a feature reads with `config.setting`, in
//! groups that go together. A server with only part of a group set refuses to start.
use crate::{Result, foundation::config::Config, manifest::Registry};

impl Registry {
    /// Refuses a server with only part of a feature's group of variables set: a setup left
    /// half done is a mistake to fix before the server starts, not one to find later. The
    /// log names the variables missing, never a value.
    pub fn check_env(&self, config: &Config) -> Result<()> {
        for feature in self.features {
            for group in feature.env {
                let missing: Vec<String> = group
                    .iter()
                    .filter(|name| config.setting(name).is_none())
                    .map(|name| config.setting_variable(name))
                    .collect();
                if !missing.is_empty() && missing.len() < group.len() {
                    crate::foundation::observability::event(
                        "error",
                        "feature_env_incomplete",
                        serde_json::json!({"feature": feature.name, "missing": missing}),
                    );
                    return Err(crate::Error::new("feature_env_incomplete", 400));
                }
            }
        }
        Ok(())
    }
}

/// Panics unless every variable is named UPPER_CASE, by one feature.
pub(crate) fn check(registry: &Registry) {
    let mut named = std::collections::BTreeSet::new();
    for feature in registry.features {
        for variable in feature.env.iter().flat_map(|group| group.iter()) {
            assert!(
                !variable.is_empty()
                    && variable
                        .bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_'),
                "{} names an environment variable that isn't UPPER_CASE: {variable}",
                feature.name
            );
            assert!(
                named.insert(*variable),
                "{} names an environment variable another feature reads: {variable}",
                feature.name
            );
        }
    }
}
