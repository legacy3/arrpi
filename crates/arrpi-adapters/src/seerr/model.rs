//! Minimal versioned Seerr projections; native user objects never leave this adapter.
use arrpi_core::{error::ApiError, source::SourceRead};
const TMDB_ORIGIN: &str = "https://www.themoviedb.org";

use serde_json::{Value, json};
pub(crate) fn id(value: &Value, name: &str) -> Result<u64, ApiError> {
    value
        .get(name)
        .and_then(Value::as_u64)
        .filter(|id| *id > 0)
        .ok_or(ApiError::INVALID_RESPONSE)
}

pub(crate) fn media(value: &Value) -> Result<Value, ApiError> {
    let id = id(value, "id")?;
    let title = value
        .get("title")
        .or_else(|| value.get("name"))
        .and_then(Value::as_str)
        .filter(|s| s.len() <= 1024)
        .ok_or(ApiError::INVALID_RESPONSE)?;
    let kind =
        value
            .get("mediaType")
            .and_then(Value::as_str)
            .unwrap_or(if value.get("name").is_some() {
                "tv"
            } else {
                "movie"
            });

    if !matches!(kind, "tv" | "movie") {
        return Err(ApiError::INVALID_RESPONSE);
    }

    let mut output = json!({"tmdb_id":id,"title":title,"year":value.get("releaseDate").or_else(||value.get("firstAirDate")).and_then(Value::as_str).and_then(|date|date.get(..4)).and_then(|year|year.parse::<u32>().ok()),"media_type":kind,"release_date":value.get("releaseDate").or_else(||value.get("firstAirDate")),"seasons":value.get("seasons").and_then(Value::as_array).map(|seasons|seasons.iter().map(|s|json!({"season":s.get("seasonNumber"),"episodes":s.get("episodeCount")})).collect::<Vec<_>>()),"season_availability":value.pointer("/mediaInfo/seasons").and_then(Value::as_array).map(|seasons|seasons.iter().map(|s|{let mut result=availability(s.get("status"));result["season"]=s["seasonNumber"].clone();result}).collect::<Vec<_>>()).unwrap_or_default(),"link":format!("{TMDB_ORIGIN}/{kind}/{id}"),"reported_updated_at":value.pointer("/mediaInfo/updatedAt"),"freshness":Value::Null});

    let overall = availability(value.pointer("/mediaInfo/status"));

    output["availability"] = overall.clone();
    output
        .as_object_mut()
        .ok_or(ApiError::INTERNAL)?
        .extend(overall.as_object().ok_or(ApiError::INTERNAL)?.clone());
    Ok(output)
}

pub(crate) fn receipt(value: &Value, actor: u64) -> Result<Value, ApiError> {
    if value.pointer("/requestedBy/id").and_then(Value::as_u64) != Some(actor) {
        return Err(ApiError::FORBIDDEN);
    }

    let id = id(value, "id")?;
    let status = value
        .get("status")
        .and_then(Value::as_u64)
        .filter(|s| (1..=5).contains(s))
        .ok_or(ApiError::INVALID_RESPONSE)?;
    let seasons = value
        .get("seasons")
        .and_then(Value::as_array)
        .map(|s| {
            s.iter()
                .filter_map(|s| s.get("seasonNumber").and_then(Value::as_u64))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let mut result = json!({"request_id":id,"status":match status{1=>"pending",2=>"approved",3=>"declined",4=>"failed",_=>"completed"},"tmdb_id":value.pointer("/media/tmdbId"),"media_type":value.get("type"),"seasons":seasons,"is_4k":value.get("is4k"),"created_at":optional_text(value.get("createdAt"),64),"updated_at":optional_text(value.get("updatedAt"),64)});

    with_media(&mut result, value.get("media").unwrap_or(&Value::Null));
    Ok(result)
}

pub(crate) fn with_media(result: &mut Value, metadata: &Value) {
    result["title"] = json!(optional_text(
        metadata.get("title").or_else(|| metadata.get("name")),
        1024
    ));
    result["release_date"] = json!(optional_text(
        metadata
            .get("releaseDate")
            .or_else(|| metadata.get("firstAirDate")),
        64
    ));
}

fn optional_text(value: Option<&Value>, maximum: usize) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .filter(|value| value.len() <= maximum)
}

fn availability(status: Option<&Value>) -> Value {
    let status = status.and_then(Value::as_u64).unwrap_or(1);

    json!({"status":status,"status_name":match status{2=>"pending",3=>"processing",4=>"partially_available",5=>"available",6=>"blocked",7=>"deleted",_=>"unknown"},"available":status==5,"partially_available":status==4,"requested":matches!(status,2|3)})
}

pub(crate) fn freshness(value: &SourceRead<Value>, bypass: bool) -> Value {
    value.freshness.public(bypass)
}

#[cfg(test)]
mod tests;
