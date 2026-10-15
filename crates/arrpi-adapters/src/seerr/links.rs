//! Public database and playback links use source IDs and Seerr's configured web URLs.
use super::{Backend, model};
const TMDB: &str = "https://www.themoviedb.org";
const IMDB: &str = "https://www.imdb.com/title";
const TRAKT: &str = "https://trakt.tv";
const LETTERBOXD: &str = "https://letterboxd.com/tmdb";
const TVDB: &str = "https://www.thetvdb.com";
const SIMKL_REDIRECT: &str = "https://api.simkl.com/redirect";
use arrpi_core::{error::ApiError, policy::Binding};
use reqwest::Method;
use serde_json::{Value, json};
impl Backend {
    pub(crate) async fn settings(&self, binding: &Binding) -> Value {
        match self
            .call(binding, Method::GET, "settings/public", &[], None)
            .await
        {
            Ok(value) => value.data,
            Err(error) => json!({"_arrpi_error":error.code()}),
        }
    }
}
pub(crate) fn media(
    value: &arrpi_core::source::SourceRead<Value>,
    settings: &Value,
    bypass: bool,
) -> Result<Value, ApiError> {
    let mut result = model::media(value)?;
    let id = model::id(value, "id")?;
    let kind = result["media_type"]
        .as_str()
        .ok_or(ApiError::INVALID_RESPONSE)?;
    let imdb = value
        .pointer("/externalIds/imdbId")
        .and_then(Value::as_str)
        .filter(|id| valid_imdb(id));
    let tvdb = value
        .pointer("/externalIds/tvdbId")
        .and_then(Value::as_u64)
        .filter(|id| *id > 0);
    let seerr = settings
        .get("applicationUrl")
        .and_then(Value::as_str)
        .and_then(|base| entry(base, kind, id));
    let playable = matches!(
        value.pointer("/mediaInfo/status").and_then(Value::as_u64),
        Some(4 | 5)
    );
    let watch = if playable {
        value
            .pointer("/mediaInfo/mediaUrl")
            .and_then(Value::as_str)
            .and_then(public_url)
            .map(|u| u.to_string())
    } else {
        None
    };
    let mut links = json!({"seerr":seerr,"watch":watch,"tmdb":format!("{TMDB}/{kind}/{id}"),
        "imdb":imdb.map(|id|format!("{IMDB}/{id}")),
        "trakt":imdb.map(|id|format!("{TRAKT}/{}/{id}",if kind=="movie" {"movies"} else {"shows"})),
        "simkl":imdb.map(|id|format!("{SIMKL_REDIRECT}?to=Simkl&imdb={id}")),
        "letterboxd":(kind=="movie").then(||format!("{LETTERBOXD}/{id}")),
        "tvdb":if kind=="tv" {tvdb.map(|id|format!("{TVDB}/?tab=series&id={id}"))} else {None}});

    if watch.is_none() {
        links["watch_unavailable_reason"] = json!(if playable {
            "Seerr has no public playback URL for this item"
        } else {
            "Media is not available in Seerr"
        });
    }

    if seerr.is_none() {
        links["seerr_unavailable_reason"] = json!(if settings.get("_arrpi_error").is_some() {
            "Seerr public settings are unavailable"
        } else {
            "Set a public Application URL in Seerr"
        });
    }

    result["links"] = links;
    result["freshness"] = value.freshness.public(bypass);
    Ok(result)
}

fn valid_imdb(id: &str) -> bool {
    id.len() <= 32
        && id
            .strip_prefix("tt")
            .is_some_and(|digits| !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
}

fn entry(base: &str, kind: &str, id: u64) -> Option<String> {
    let mut base = public_url(base)?;

    if base.query().is_some() || base.fragment().is_some() {
        return None;
    }

    let path = format!("{}/", base.path().trim_end_matches('/'));

    base.set_path(&path);
    base.join(&format!("{kind}/{id}"))
        .ok()
        .map(|u| u.to_string())
}

fn public_url(value: &str) -> Option<url::Url> {
    if value.len() > 4096 {
        return None;
    }

    let url = url::Url::parse(value).ok()?;

    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }

    let public = match url.host()? {
        url::Host::Ipv4(ip) => public_ipv4(ip),

        url::Host::Ipv6(ip) => ip.to_ipv4().map_or_else(
            || {
                !(ip.is_loopback()
                    || ip.is_unspecified()
                    || ip.is_unique_local()
                    || ip.is_unicast_link_local()
                    || ip.is_multicast())
            },
            public_ipv4,
        ),

        url::Host::Domain(host) => {
            host.contains('.')
                && ![".internal", ".local", ".localhost"]
                    .iter()
                    .any(|suffix| host.ends_with(suffix))
        }
    };

    if !public
        || secret_query(url.query().unwrap_or_default())
        || url
            .fragment()
            .and_then(|f| f.split_once('?'))
            .is_some_and(|(_, q)| secret_query(q))
    {
        return None;
    }

    Some(url)
}

fn secret_query(query: &str) -> bool {
    url::form_urlencoded::parse(query.as_bytes()).any(|(key, _)| {
        matches!(
            key.to_ascii_lowercase().as_str(),
            "token"
                | "apikey"
                | "api_key"
                | "access_token"
                | "x-plex-token"
                | "authorization"
                | "password"
        )
    })
}

fn public_ipv4(ip: std::net::Ipv4Addr) -> bool {
    let bytes = ip.octets();

    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_documentation()
        || (bytes[0] == 100 && (64..=127).contains(&bytes[1])))
}
