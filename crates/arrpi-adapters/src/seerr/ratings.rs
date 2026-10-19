//! Seerr's TMDB metadata and separate IMDb/Rotten Tomatoes responses.
use super::{Backend, model};
use arrpi_core::{
    error::ApiError,
    policy::Binding,
    ratings::{self, Origin, Provider, Rating},
    source::SourceRead,
};
use reqwest::Method;
use serde_json::{Value, json};

pub(super) fn metadata(value: &Value, output: &mut Value) -> Result<(), ApiError> {
    let mut ratings = Vec::new();
    let votes = value.get("voteCount").and_then(Value::as_u64);

    if votes != Some(0)
        && let Some(score) = value.get("voteAverage").and_then(Value::as_f64)
    {
        push(&mut ratings, Provider::Tmdb, score, 10.0, votes);
    }

    output["overview"] = text(value.get("overview"), 8000);
    output["original_title"] = text(
        value
            .get("originalTitle")
            .or_else(|| value.get("originalName")),
        1024,
    );
    output["original_language"] = text(value.get("originalLanguage"), 64);
    output["runtime_minutes"] = json!(value.get("runtime").and_then(Value::as_u64).or_else(|| {
        value
            .get("episodeRunTime")
            .and_then(Value::as_array)
            .and_then(|values| values.first())
            .and_then(Value::as_u64)
    }));
    output["genres"] = json!(
        value
            .get("genres")
            .and_then(Value::as_array)
            .map(|genres| genres
                .iter()
                .take(32)
                .filter_map(|genre| genre
                    .get("name")
                    .and_then(Value::as_str)
                    .filter(|name| name.len() <= 256))
                .collect::<Vec<_>>())
            .unwrap_or_default()
    );
    output["imdb_id"] = text(
        value
            .pointer("/externalIds/imdbId")
            .or_else(|| value.get("imdbId")),
        32,
    );
    output["tvdb_id"] = json!(value.pointer("/externalIds/tvdbId").and_then(Value::as_u64));
    assign(output, &ratings)
}

fn text(value: Option<&Value>, limit: usize) -> Value {
    json!(
        value
            .and_then(Value::as_str)
            .filter(|value| value.len() <= limit)
    )
}

fn push(ratings: &mut Vec<Rating>, provider: Provider, value: f64, scale: f64, votes: Option<u64>) {
    if let Some(rating) = Rating::new(provider, Origin::Seerr, value, scale, votes) {
        ratings.push(rating);
    }
}

pub(super) fn assign(output: &mut Value, ratings: &[Rating]) -> Result<(), ApiError> {
    let primary = ratings::community(ratings);

    output["rating"] = json!(primary.map(|rating| rating.score));
    output["rating_provider"] = json!(primary.map(|rating| rating.provider));
    output["ratings"] = serde_json::to_value(ratings)?;
    Ok(())
}

impl Backend {
    pub(super) async fn community_ratings(
        &self,
        binding: &Binding,
        value: &SourceRead<Value>,
        output: &mut Value,
        bypass: bool,
    ) -> Result<(), ApiError> {
        let kind = output["media_type"]
            .as_str()
            .ok_or(ApiError::INVALID_RESPONSE)?;
        let id = model::id(value, "id")?;
        let suffix = if kind == "movie" {
            "ratingscombined"
        } else {
            "ratings"
        };
        let response = self
            .call(
                binding,
                Method::GET,
                &format!("{kind}/{id}/{suffix}"),
                &[],
                None,
            )
            .await;
        let mut ratings: Vec<Rating> = serde_json::from_value(output["ratings"].clone())?;

        match response {
            Ok(response) => {
                append_external(&response.data, &mut ratings);

                output["rating_sources"] = json!([{"source":"seerr","status":"ok","freshness":response.freshness.public(bypass)}]);
            }

            Err(error) => {
                output["rating_sources"] =
                    json!([{"source":"seerr","status":"unavailable","code":error.code()}]);
            }
        }

        output["ratings_complete"] = json!(output["rating_sources"][0]["status"] == "ok");
        assign(output, &ratings)
    }
}

fn append_external(response: &Value, ratings: &mut Vec<Rating>) {
    if let Some(score) = response
        .pointer("/imdb/criticsScore")
        .and_then(Value::as_f64)
    {
        push(ratings, Provider::Imdb, score, 10.0, None);
    }

    let rt = response.get("rt").unwrap_or(response);

    for (field, provider) in [
        ("criticsScore", Provider::RottenTomatoes),
        ("audienceScore", Provider::RottenTomatoesAudience),
    ] {
        if let Some(score) = rt.get(field).and_then(Value::as_f64) {
            push(ratings, provider, score, 100.0, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_and_external_scores_preserve_scales_and_missing_values() {
        let mut output = json!({});

        metadata(
            &json!({"voteAverage":7.8,"voteCount":24000,"genres":[{"name":"Comedy"}],"runtime":98}),
            &mut output,
        )
        .unwrap();
        let mut ratings: Vec<Rating> = serde_json::from_value(output["ratings"].clone()).unwrap();

        append_external(
            &json!({"imdb":{"criticsScore":8.2},"rt":{"criticsScore":0,"audienceScore":87}}),
            &mut ratings,
        );
        assign(&mut output, &ratings).unwrap();
        assert_eq!(output["rating_provider"], "imdb");
        assert_eq!(output["ratings"][0]["votes"], 24000);
        assert_eq!(output["ratings"][2]["raw_scale"], 100.0);
        assert_eq!(output["ratings"][2]["score"], 0.0);
        assert_eq!(output["ratings"][3]["score"], 8.7);
        assert_eq!(output["ratings"][1]["origin"], "seerr");
        let mut absent = Vec::new();

        append_external(
            &json!({"criticsScore":null,"audienceScore":null}),
            &mut absent,
        );
        assert!(absent.is_empty());
        append_external(&json!({"criticsScore":91,"audienceScore":102}), &mut absent);
        assert_eq!(absent.len(), 1);
        assert_eq!(absent[0].provider, Provider::RottenTomatoes);
        assert!((absent[0].score - 9.1).abs() < f64::EPSILON);
        metadata(&json!({"voteAverage":0,"voteCount":0}), &mut output).unwrap();
        assert_eq!(output["ratings"], json!([]));
        assert!(output["rating"].is_null());
    }
}
