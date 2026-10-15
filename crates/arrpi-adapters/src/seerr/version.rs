//! Stable compatible updates retain writes; native actor checks still run per operation.
use semver::Version;

pub(super) fn supports_writes(version: Option<&str>) -> bool {
    version
        .and_then(|value| Version::parse(value.strip_prefix('v').unwrap_or(value)).ok())
        .is_some_and(|value| value.major == 3 && value.minor >= 5 && value.pre.is_empty())
}

#[cfg(test)]
mod tests {
    use super::supports_writes;

    #[test]
    fn compatible_stable_updates_do_not_require_an_adapter_release() {
        for version in [
            "3.5.0",
            "3.5.1",
            "3.6.0",
            "3.99.99",
            "v3.5.1",
            "3.5.1+container.2",
        ] {
            assert!(supports_writes(Some(version)), "{version}");
        }
    }

    #[test]
    fn unknown_older_breaking_or_prerelease_contracts_remain_read_only() {
        for version in [
            None,
            Some(""),
            Some("untested"),
            Some("3.5"),
            Some("3.4.99"),
            Some("4.0.0"),
            Some("2.9.0"),
            Some("3.6.0-rc.1"),
        ] {
            assert!(!supports_writes(version), "{version:?}");
        }
    }
}
