//! Real JWT signatures, HTTP discovery and JWKS with fake external authorities.

use arrpi_core::{
    config::Config,
    oauth::{Binding, Policy},
};
use oauth_resource_server::testing::TestAuthority;

use super::Verifier;

const RESOURCE: &str = "https://arrpi.example.test/v1/mcp";

fn config(issuer: &str) -> Config {
    let mut config =
        Config::parse(include_str!("../../../../tests/arrpi.json")).expect("fake settings");

    config.mcp.public_url = Some(RESOURCE.into());
    config.oauth = Some(Policy {
        enabled: true,
        issuer: issuer.into(),
        required_scope: "arrpi:mcp".into(),
        bindings: vec![Binding {
            issuer: issuer.into(),
            subject: "alice".into(),
            client: "requester-1".into(),
        }],
    });
    config.validate().expect("valid OAuth settings");
    config
}

#[tokio::test]
async fn discovery_verifies_audience_scope_and_explicit_subject_without_claim_grants() {
    let authority = TestAuthority::start().await;
    let configured = config(authority.issuer());
    let verifier = Verifier::new(&configured)
        .expect("verifier")
        .expect("enabled");
    let token = authority
        .token()
        .subject("alice")
        .claim(
            "aud",
            serde_json::json!([RESOURCE, "https://another.example.test"]),
        )
        .scopes(["arrpi:mcp"])
        .claim("roles", ["admin"])
        .sign();

    assert_eq!(
        verifier.client(Some(&token)).await.expect("mapped caller"),
        "requester-1"
    );
    assert_eq!(authority.discovery_fetches(), 1);
    assert_eq!(authority.jwks_fetches(), 1);
    assert_eq!(verifier.metadata()["resource"], RESOURCE);
    assert_eq!(
        verifier.metadata()["authorization_servers"][0],
        authority.issuer()
    );
    assert_eq!(
        verifier.metadata_path(),
        "/.well-known/oauth-protected-resource/v1/mcp"
    );
    assert!(
        verifier
            .missing()
            .challenge
            .expect("challenge")
            .contains("resource_metadata=")
    );
    assert_eq!(
        verifier.client(Some(&token)).await.expect("warm caller"),
        "requester-1"
    );
    assert_eq!(
        authority.jwks_fetches(),
        1,
        "verification reuses bounded signing keys"
    );
}

#[tokio::test]
async fn invalid_tokens_id_tokens_and_unmapped_identities_fail_closed() {
    let authority = TestAuthority::start().await;
    let configured = config(authority.issuer());
    let verifier = Verifier::new(&configured)
        .expect("verifier")
        .expect("enabled");
    let good = || {
        authority
            .token()
            .subject("alice")
            .audience(RESOURCE)
            .scopes(["arrpi:mcp"])
    };
    let cases = [
        (
            good().audience("https://other.example.test/v1/mcp").sign(),
            401,
        ),
        (
            good().issuer("https://other-issuer.example.test").sign(),
            401,
        ),
        (good().expired().sign(), 401),
        (good().not_before_in(120).sign(), 401),
        (good().typ("JWT").sign(), 401),
        (good().without_claim("sub").sign(), 401),
        (good().subject("").sign(), 401),
        (
            good().subject("not-bound").claim("email", "alice").sign(),
            403,
        ),
        (good().scopes(["admin"]).sign(), 403),
        (good().issued_ago(7200).sign(), 401),
        (good().without_claim("iat").sign(), 401),
        ("oversized".repeat(2200), 401),
    ];

    for (token, status) in cases {
        let refusal = verifier.client(Some(&token)).await.expect_err("refused");

        assert_eq!(refusal.status, status);
        assert!(
            !refusal
                .challenge
                .as_deref()
                .unwrap_or_default()
                .contains(&token)
        );
    }

    let absent = verifier.client(None).await.expect_err("missing credential");

    assert_eq!(absent.status, 401);
    assert!(
        absent
            .challenge
            .expect("challenge")
            .contains("scope=\"arrpi:mcp\"")
    );
}

#[tokio::test]
async fn rebinding_revocation_and_issuer_changes_apply_on_the_next_revision() {
    let authority = TestAuthority::start().await;
    let mut configured = config(authority.issuer());
    let token = authority
        .token()
        .subject("alice")
        .audience(RESOURCE)
        .scopes(["arrpi:mcp"])
        .sign();
    let first = Verifier::new(&configured)
        .expect("verifier")
        .expect("enabled");

    assert_eq!(
        first.client(Some(&token)).await.expect("first mapping"),
        "requester-1"
    );
    configured.oauth.as_mut().expect("policy").bindings[0].client = "requester-2".into();
    let rebound = Verifier::new(&configured)
        .expect("verifier")
        .expect("enabled");

    assert_eq!(
        rebound.client(Some(&token)).await.expect("new mapping"),
        "requester-2"
    );
    configured.oauth.as_mut().expect("policy").bindings.clear();
    let revoked = Verifier::new(&configured)
        .expect("verifier")
        .expect("enabled");

    assert_eq!(
        revoked
            .client(Some(&token))
            .await
            .expect_err("revoked")
            .status,
        403
    );
    let other = TestAuthority::start().await;
    let other_token = other
        .token()
        .subject("alice")
        .audience(RESOURCE)
        .scopes(["arrpi:mcp"])
        .sign();

    assert_eq!(
        first
            .client(Some(&other_token))
            .await
            .expect_err("different issuer")
            .status,
        401
    );
}

#[tokio::test]
async fn unknown_signing_keys_do_not_trigger_unbounded_refreshes() {
    let authority = TestAuthority::start().await;
    let configured = config(authority.issuer());
    let verifier = Verifier::new(&configured)
        .expect("verifier")
        .expect("enabled");
    let good = authority
        .token()
        .subject("alice")
        .audience(RESOURCE)
        .scopes(["arrpi:mcp"])
        .sign();

    verifier.client(Some(&good)).await.expect("initial keys");

    for kid in ["unknown-a", "unknown-b", "unknown-c"] {
        let token = authority
            .token()
            .subject("alice")
            .audience(RESOURCE)
            .scopes(["arrpi:mcp"])
            .kid(kid)
            .sign();

        assert_eq!(
            verifier
                .client(Some(&token))
                .await
                .expect_err("unknown key")
                .status,
            401
        );
    }

    assert_eq!(
        authority.jwks_fetches(),
        1,
        "unknown kid storms respect library cooldown"
    );
    authority.rotate_key();
    let fresh = Verifier::new(&configured)
        .expect("replacement verifier")
        .expect("enabled");
    let rotated = authority
        .token()
        .subject("alice")
        .audience(RESOURCE)
        .scopes(["arrpi:mcp"])
        .sign();

    assert_eq!(
        fresh
            .client(Some(&rotated))
            .await
            .expect("rotated signature"),
        "requester-1"
    );
}
