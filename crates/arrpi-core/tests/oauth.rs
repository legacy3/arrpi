//! OAuth never grants more access than an explicitly bound existing requester.

use arrpi_core::{
    config::Config,
    oauth::{Binding, Policy},
};

fn configured() -> Config {
    let mut config =
        Config::parse(include_str!("../../../tests/arrpi.json")).expect("fake settings");

    config.mcp.public_url = Some("https://arrpi.example.test/v1/mcp".into());
    config.oauth = Some(Policy {
        enabled: true,
        issuer: "https://identity.example.test/realms/media".into(),
        required_scope: "arrpi:mcp".into(),
        bindings: vec![Binding {
            issuer: "https://identity.example.test/realms/media".into(),
            subject: "stable-user-uuid".into(),
            client: "requester-1".into(),
        }],
    });
    config
}

#[test]
fn oauth_configuration_requires_a_ready_public_mcp_and_explicit_non_admin_bindings() {
    let config = configured();

    config.validate().expect("valid explicit binding");

    for client in ["missing-client", "reader", "calendar"] {
        let mut candidate = configured();

        candidate.oauth.as_mut().expect("policy").bindings[0].client = client.into();
        candidate
            .validate()
            .expect_err("unknown, privileged or unbound client");
    }

    let mut duplicate = configured();
    let binding = duplicate.oauth.as_ref().expect("policy").bindings[0].clone();

    duplicate
        .oauth
        .as_mut()
        .expect("policy")
        .bindings
        .push(binding);
    duplicate.validate().expect_err("duplicate identity");
    let mut wrong_issuer = configured();

    wrong_issuer.oauth.as_mut().expect("policy").bindings[0].issuer =
        "https://other.example.test/".into();
    wrong_issuer
        .validate()
        .expect_err("issuer change must not silently rebind subjects");
    let mut no_public_endpoint = configured();

    no_public_endpoint.mcp.public_url = None;
    no_public_endpoint
        .validate()
        .expect_err("public endpoint required");
}

#[test]
fn oauth_policy_refuses_decorated_insecure_or_ambiguous_issuer_and_scope_values() {
    for issuer in [
        "http://remote.example.test/",
        "https:/broken.example.test",
        "https://user:password@example.test/",
        "https://example.test/?token=private",
        "https://example.test/#fragment",
    ] {
        let mut config = configured();
        let policy = config.oauth.as_mut().expect("policy");

        policy.issuer = issuer.into();
        policy.bindings[0].issuer = issuer.into();
        config.validate().expect_err("invalid issuer or scope");
    }

    for scope in ["", "two scopes", "bad\"scope", "bad\\scope"] {
        let mut config = configured();

        config.oauth.as_mut().expect("policy").required_scope = scope.into();
        config.validate().expect_err("invalid issuer or scope");
    }

    let debug = format!("{:?}", configured().oauth);

    assert!(!debug.contains("identity.example.test") && !debug.contains("stable-user-uuid"));
}
