//! Playback URLs are native Seerr web URLs; missing metadata never invents a watch link.
use crate::seerr::links;
use arrpi_core::source::{Freshness, Observation, SourceRead};
use serde_json::{Value, json};
#[test]
fn public_links_use_configured_base_path_and_known_external_identifiers() {
    let value = json!({"id":42,"title":"Movie","externalIds":{"imdbId":"tt1234567","tvdbId":99},"mediaInfo":{"status":5,"mediaUrl":"https://watch.example.test/web#!/server/machine/details?key=%2Flibrary%2Fmetadata%2F55","serviceUrl":"http://radarr:7878/private","plexToken":"secret"},"_arrpi_freshness":{"cache":"not_cached"}});
    let output = media(
        &value,
        &json!({"applicationUrl":"https://request.example.test/base"}),
        true,
    )
    .expect("links");

    assert_eq!(
        output["links"]["seerr"],
        "https://request.example.test/base/movie/42"
    );
    assert_eq!(output["links"]["watch"], value["mediaInfo"]["mediaUrl"]);
    assert_eq!(
        output["links"]["trakt"],
        "https://trakt.tv/movies/tt1234567"
    );
    assert_eq!(
        output["links"]["letterboxd"],
        "https://letterboxd.com/tmdb/42"
    );
    assert_eq!(output["links"]["tvdb"], Value::Null);
    assert!(!output.to_string().contains("secret") && !output.to_string().contains("radarr:7878"));
    assert_eq!(output["freshness"]["bypass_requested"], true);
}

#[test]
fn unavailable_private_credentialed_and_invalid_urls_are_never_returned() {
    let mut value = json!({"id":42,"name":"Show","externalIds":{"imdbId":"tt1&secret=bad","tvdbId":99},"mediaInfo":{"status":4},"_arrpi_freshness":{"cache":"not_cached"}});

    for url in [
        "http://plex:32400/web",
        "http://127.0.0.1/web",
        "http://192.168.1.2/web",
        "http://[::ffff:192.168.1.2]/web",
        "http://100.100.10.20/web",
        "https://user:password@watch.example.test/",
        "https://watch.example.test/?X-Plex-Token=secret",
        "https://watch.example.test/#!/server?api_key=secret",
        "javascript:alert(1)",
    ] {
        value["mediaInfo"]["mediaUrl"] = json!(url);
        let output = media(&value, &json!({}), false).expect("links");

        assert_eq!(output["links"]["watch"], Value::Null);
        assert_eq!(output["links"]["seerr"], Value::Null);
        assert_eq!(output["links"]["imdb"], Value::Null);
        assert_eq!(
            output["links"]["tvdb"],
            "https://www.thetvdb.com/?tab=series&id=99"
        );
    }

    value["mediaInfo"]["mediaUrl"] = json!("https://watch.example.test/web");
    value["mediaInfo"]["status"] = json!(1);
    let output = media(
        &value,
        &json!({"_arrpi_error":"upstream_unavailable"}),
        false,
    )
    .expect("links");

    assert_eq!(output["links"]["watch"], Value::Null);
    assert_eq!(output["links"]["letterboxd"], Value::Null);
}

fn media(
    value: &Value,
    settings: &Value,
    bypass: bool,
) -> Result<Value, arrpi_core::error::ApiError> {
    links::media(
        &SourceRead {
            data: value.clone(),
            freshness: Freshness {
                source: "seerr".to_owned(),
                cache: Observation::NotCached,
                age_seconds: None,
                checked_at: None,
                note: None,
            },
        },
        settings,
        bypass,
    )
}
