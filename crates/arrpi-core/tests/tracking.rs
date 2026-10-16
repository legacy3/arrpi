//! Tracking identity bindings remain unique and separate from configuration access.

use arrpi_core::config::Config;
use serde_json::{Value, json};

fn settings() -> Value {
    json!({"listen":"127.0.0.1:0","sources":{"crosswatch":{"kind":"crosswatch","url":"http://crosswatch:8787/","api_key":"fake-native-credential"}},"clients":[{"name":"alice","key":"fake-alice-00000000000000000000000","permissions":["crosswatch:tracking-view"],"tracking_user":{"source":"crosswatch","profile_id":"profile-alice","plex_id":42}}]})
}

#[test]
fn duplicate_native_profile_or_plex_identity_is_rejected() {
    for same_profile in [false, true] {
        let mut document = settings();
        let mut second = document["clients"][0].clone();

        second["name"] = json!("bob");
        second["key"] = json!("fake-bob-0000000000000000000000000");
        second["tracking_user"]["profile_id"] = json!(if same_profile {
            "profile-alice"
        } else {
            "profile-bob"
        });
        second["tracking_user"]["plex_id"] = json!(if same_profile { 43 } else { 42 });
        document["clients"]
            .as_array_mut()
            .expect("clients")
            .push(second);
        let config: Config = serde_json::from_value(document).expect("settings shape");

        assert!(
            config.validate().is_err(),
            "reused native identity is rejected"
        );
    }
}

#[test]
fn tracking_does_not_require_seerr_but_cannot_share_configuration_authority() {
    let mut document = settings();
    let config: Config = serde_json::from_value(document.clone()).expect("settings shape");

    config.validate().expect("tracking without Seerr");
    document["clients"][0]["permissions"] = json!(["admin"]);
    let privileged: Config = serde_json::from_value(document).expect("settings shape");

    assert!(
        privileged.validate().is_err(),
        "administrator cannot impersonate a tracking account"
    );
}
