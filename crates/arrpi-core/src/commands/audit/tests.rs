//! Audit distinguishes dispatched episode scope from the original request on uncertain writes.

use super::{merge, target};
use serde_json::json;

#[test]
fn confirmed_episode_scope_replaces_requested_ids_but_failure_retains_request() {
    let input = target(&json!({"item_id":7,"episode_ids":[1,2,3,4,5,6]}));
    let receipt = json!({"source":"shows","item_id":7,"command_id":77,"episode_ids":[1,6],"missing_only":true});
    let confirmed = merge(input.clone(), &receipt);

    assert_eq!(confirmed.episode_ids, Some(vec![1, 6]));
    assert_eq!(confirmed.source.as_deref(), Some("shows"));
    assert_eq!(confirmed.command_id, Some(77));
    assert_eq!(confirmed.missing_only, Some(true));
    let failed = merge(input, &json!({"code":"unknown_outcome"}));

    assert_eq!(failed.episode_ids, Some(vec![1, 2, 3, 4, 5, 6]));
    let season = merge(target(&json!({"item_id":7,"season":1})), &receipt);

    assert_eq!(season.episode_ids, Some(vec![1, 6]));
    assert_eq!(season.season, Some(1));
}
