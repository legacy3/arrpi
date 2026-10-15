//! Stdio metadata retains only the last caller's advertised central schema.

use arrpi_core::{executor::CommandExecutor, registry::ToolId};
use serde_json::json;

use crate::remote::RemoteApi;

use super::{CLIENT_KEY, fixture, response};

#[tokio::test]
async fn discovered_nondefault_limits_are_caller_scoped_without_fallbacks() {
    let payload = json!({"tools":[{
        "name":"list_movies",
        "description":"configured central library",
        "inputSchema":{
            "type":"object",
            "properties":{"limit":{"type":"integer","maximum":7}}
        }
    }]});
    let (server, _calls) = fixture(response(200, &payload.to_string(), None)).await;
    let remote = RemoteApi::new(&server.url, false).expect("remote");

    assert!(
        remote
            .definition(Some(CLIENT_KEY), ToolId::ListMovies)
            .is_none()
    );
    let listed = remote.list(CLIENT_KEY).await.expect("advertised schema");
    let definition = remote
        .definition(Some(CLIENT_KEY), ToolId::ListMovies)
        .expect("last caller metadata");

    assert_eq!(
        definition.input_schema().as_ref()["properties"]["limit"]["maximum"],
        7
    );
    assert_eq!(
        definition.input_schema().as_ref(),
        listed[0].input_schema().as_ref()
    );
    assert!(remote.definition(None, ToolId::ListMovies).is_none());
    assert!(
        remote
            .definition(Some("second-key"), ToolId::ListMovies)
            .is_none()
    );
    assert!(
        remote
            .definition(Some(CLIENT_KEY), ToolId::MyQuota)
            .is_none()
    );
    remote.list("second-key").await.expect("second discovery");
    assert!(
        remote
            .definition(Some(CLIENT_KEY), ToolId::ListMovies)
            .is_none()
    );
    assert!(
        remote
            .definition(Some("second-key"), ToolId::ListMovies)
            .is_some()
    );
    assert!(!format!("{remote:?}").contains(CLIENT_KEY));
    assert!(!format!("{remote:?}").contains("second-key"));
    remote.list("").await.expect_err("invalid rediscovery");
    assert!(
        remote
            .definition(Some("second-key"), ToolId::ListMovies)
            .is_none()
    );
}

#[tokio::test]
async fn duplicate_advertisements_cannot_choose_ambiguous_tool_metadata() {
    let tool = json!({"name":"my_quota","inputSchema":{"type":"object"}});
    let payload = json!({"tools":[tool.clone(),tool]});
    let (server, _calls) = fixture(response(200, &payload.to_string(), None)).await;
    let remote = RemoteApi::new(&server.url, false).expect("remote");
    let error = remote.list(CLIENT_KEY).await.expect_err("ambiguous tools");

    assert_eq!(error.code(), "invalid_upstream_response");
    assert!(
        remote
            .definition(Some(CLIENT_KEY), ToolId::MyQuota)
            .is_none()
    );
}
