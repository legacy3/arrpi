//! Tracking routes cannot inherit administrator, requester or browser identity.

use std::sync::atomic::Ordering;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt as _;

use super::{CLIENT_KEY, gateway, mock};
use crate::proxy::router;

#[tokio::test]
async fn unbound_and_forged_tracking_requests_never_reach_native_admin_api() {
    let (server, native) = mock("{}").await;
    let app = router(gateway(&server.url));

    for (key, method, path, body, expected) in [
        (None, "GET", "/v1/tracking", "", StatusCode::UNAUTHORIZED),
        (
            Some(CLIENT_KEY),
            "GET",
            "/v1/tracking",
            "",
            StatusCode::FORBIDDEN,
        ),
        (
            Some(CLIENT_KEY),
            "POST",
            "/v1/tracking/enable",
            "{}",
            StatusCode::FORBIDDEN,
        ),
        (
            Some(CLIENT_KEY),
            "POST",
            "/v1/tracking/enable",
            r#"{"profile_id":"victim"}"#,
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("content-type", "application/json")
            .header("x-authentik-uid", "victim")
            .header("x-arrpi-subject", "victim");

        if let Some(key) = key {
            request = request.header("authorization", format!("Bearer {key}"));
        }

        let response = app
            .clone()
            .oneshot(request.body(Body::from(body)).expect("request"))
            .await
            .expect("response");

        assert_eq!(response.status(), expected, "{method} {path}");
    }

    assert_eq!(
        native.calls.load(Ordering::Relaxed),
        0,
        "all unbound clients denied before native calls"
    );
}
