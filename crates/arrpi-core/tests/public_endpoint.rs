//! Public endpoint validation is shared without exposing protocol types.

use arrpi_core::{config::https_endpoint, policy::Policy};

#[test]
fn public_endpoints_preserve_exact_path_authority_and_origin() {
    for (value, authority, origin) in [
        (
            "https://arrpi.example.invalid/v1/mcp",
            "arrpi.example.invalid",
            "https://arrpi.example.invalid",
        ),
        (
            "https://arrpi.example.invalid:8443/v1/mcp",
            "arrpi.example.invalid:8443",
            "https://arrpi.example.invalid:8443",
        ),
        (
            "https://[::1]:8443/v1/mcp",
            "[::1]:8443",
            "https://[::1]:8443",
        ),
    ] {
        let endpoint = https_endpoint(value, "/v1/mcp").expect("exact HTTPS endpoint");

        assert_eq!(endpoint.authority(), authority);
        assert_eq!(endpoint.origin(), origin);
        assert_eq!(format!("{endpoint:?}"), "HttpsEndpoint { [REDACTED] }");
        Policy {
            public_url: Some(value.to_owned()),
            ..Policy::default()
        }
        .validate()
        .expect("policy uses shared validator");
    }

    https_endpoint("https://arrpi.example.invalid/api", "/api").expect("generic path");
}

#[test]
fn public_endpoints_reject_decoration_and_inexact_paths() {
    for value in [
        "not a URL",
        "http://arrpi.example.invalid/v1/mcp",
        "https://arrpi.example.invalid/v1/mcp/",
        "https://arrpi.example.invalid/v1/MCP",
        "https://arrpi.example.invalid/v1/%6dcp",
        "https://arrpi.example.invalid/mcp",
        "https://user@arrpi.example.invalid/v1/mcp",
        "https://user:password@arrpi.example.invalid/v1/mcp",
        "https://arrpi.example.invalid/v1/mcp?",
        "https://arrpi.example.invalid/v1/mcp#",
        "https://arrpi.example.invalid/v1/mcp?x=1",
        "https://arrpi.example.invalid/v1/mcp#fragment",
    ] {
        https_endpoint(value, "/v1/mcp").expect_err("invalid generic endpoint");
        let error = Policy {
            public_url: Some(value.to_owned()),
            ..Policy::default()
        }
        .validate()
        .expect_err("policy rejects the same endpoint");

        assert!(!error.to_string().contains(value));
    }
}
