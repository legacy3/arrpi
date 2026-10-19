//! Project only database IDs and public score fields from Plex pages.
use arrpi_core::{
    arguments::MediaType,
    error::ApiError,
    library::RatingRecord,
    ratings::{Origin, Provider, Rating},
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Envelope {
    #[serde(rename = "MediaContainer")]
    container: Container,
}
#[derive(Deserialize)]
struct Container {
    #[serde(default, rename = "Metadata")]
    metadata: Vec<Metadata>,
    offset: Option<usize>,
    #[serde(rename = "totalSize")]
    total_size: Option<usize>,
}
#[derive(Deserialize)]
struct Metadata {
    #[serde(rename = "ratingKey")]
    key: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default, rename = "Guid")]
    guids: Vec<Guid>,
    #[serde(default, rename = "Rating")]
    ratings: Vec<Score>,
    rating: Option<f64>,
    #[serde(rename = "ratingImage")]
    rating_image: Option<String>,
    #[serde(rename = "audienceRating")]
    audience_rating: Option<f64>,
    #[serde(rename = "audienceRatingImage")]
    audience_image: Option<String>,
}
#[derive(Deserialize)]
struct Guid {
    id: String,
}
#[derive(Deserialize)]
struct Score {
    image: String,
    #[serde(rename = "type")]
    kind: String,
    value: f64,
}

pub(super) struct Page {
    items: Vec<Metadata>,
    pub(super) finished: bool,
}

pub(super) fn page(body: &[u8], kind: u8, offset: usize) -> Result<Page, ApiError> {
    let container = serde_json::from_slice::<Envelope>(body)?.container;
    let count = container.metadata.len();
    let expected = if kind == 1 { "movie" } else { "show" };

    if count > 100
        || container.offset.is_some_and(|actual| actual != offset)
        || (count == 0 && container.total_size.is_some_and(|total| total > offset))
        || container.metadata.iter().any(|item| {
            item.kind != expected
                || item.key.len() > 20
                || !item.key.bytes().all(|byte| byte.is_ascii_digit())
                || item.key.parse::<u64>().map_or(true, |id| id == 0)
        })
    {
        return Err(ApiError::INVALID_RESPONSE);
    }

    let mut items = container.metadata;

    items.sort_unstable_by(|left, right| left.key.cmp(&right.key));

    if items.windows(2).any(|pair| pair[0].key == pair[1].key) {
        return Err(ApiError::INVALID_RESPONSE);
    }

    Ok(Page {
        items,
        finished: container
            .total_size
            .map_or(count < 100, |total| offset.saturating_add(count) >= total),
    })
}

impl Page {
    pub(super) fn count(&self) -> usize {
        self.items.len()
    }

    pub(super) fn metadata_path(&self) -> String {
        format!(
            "library/metadata/{}",
            self.items
                .iter()
                .map(|item| item.key.as_str())
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    pub(super) fn append(
        self,
        details: &[u8],
        kind: u8,
        output: &mut Vec<u8>,
        limit: usize,
    ) -> Result<(), ApiError> {
        let full = page(details, kind, 0)?;

        if self.items.len() != full.items.len()
            || self
                .items
                .iter()
                .zip(&full.items)
                .any(|(base, full)| base.key != full.key)
        {
            return Err(ApiError::INVALID_RESPONSE);
        }

        let media_type = if kind == 1 {
            MediaType::Movie
        } else {
            MediaType::Tv
        };

        for (base, full) in self.items.into_iter().zip(full.items) {
            let base = project(base, media_type);
            let mut record = project(full, media_type);

            record.tmdb_id = record.tmdb_id.or(base.tmdb_id);
            record.tvdb_id = record.tvdb_id.or(base.tvdb_id);
            record.imdb_id = record.imdb_id.or(base.imdb_id);

            for rating in base.ratings {
                if !record
                    .ratings
                    .iter()
                    .any(|existing| existing.provider == rating.provider)
                {
                    record.ratings.push(rating);
                }
            }

            if !record.ratings.is_empty() {
                append(&record, output, limit)?;
            }
        }

        Ok(())
    }
}

fn append(record: &RatingRecord, output: &mut Vec<u8>, limit: usize) -> Result<(), ApiError> {
    let record = serde_json::to_vec(record)?;
    let required = output.len().saturating_add(record.len()).saturating_add(2);

    if required > limit {
        return Err(ApiError::RESPONSE_TOO_LARGE);
    }

    if output.capacity() < required {
        let desired = required.max(output.capacity().saturating_mul(2)).min(limit);

        output.reserve_exact(desired - output.len());
    }

    if output.len() > 1 {
        output.push(b',');
    }

    output.extend_from_slice(&record);
    Ok(())
}

fn project(item: Metadata, media_type: MediaType) -> RatingRecord {
    let mut record = RatingRecord {
        media_type,
        tmdb_id: None,
        tvdb_id: None,
        imdb_id: None,
        ratings: Vec::new(),
    };

    for guid in item.guids {
        if let Some(id) = guid.id.strip_prefix("tmdb://") {
            record.tmdb_id = id.parse::<u64>().ok().filter(|id| *id > 0);
        }

        if let Some(id) = guid.id.strip_prefix("tvdb://") {
            record.tvdb_id = id.parse::<u64>().ok().filter(|id| *id > 0);
        }

        if let Some(id) = guid.id.strip_prefix("imdb://").filter(|id| {
            id.len() <= 32
                && id.strip_prefix("tt").is_some_and(|digits| {
                    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
                })
        }) {
            record.imdb_id = Some(id.to_owned());
        }
    }

    for score in item.ratings {
        push(&mut record.ratings, &score.image, &score.kind, score.value);
    }

    if let Some(value) = item.rating {
        push(
            &mut record.ratings,
            item.rating_image.as_deref().unwrap_or(""),
            "critic",
            value,
        );
    }

    if let Some(value) = item.audience_rating {
        push(
            &mut record.ratings,
            item.audience_image.as_deref().unwrap_or(""),
            "audience",
            value,
        );
    }

    record
}

fn push(ratings: &mut Vec<Rating>, image: &str, kind: &str, value: f64) {
    if !matches!(kind, "critic" | "audience") {
        return;
    }

    let provider = provider(image, kind);

    if ratings.iter().any(|rating| rating.provider == provider) {
        return;
    }

    if let Some(rating) = Rating::new(provider, Origin::Plex, value, 10.0, None) {
        ratings.push(rating);
    }
}

fn provider(image: &str, kind: &str) -> Provider {
    match image.split_once("://").map(|(name, _)| name) {
        Some("imdb") => Provider::Imdb,
        Some("themoviedb" | "tmdb") => Provider::Tmdb,
        Some("rottentomatoes") if kind == "audience" => Provider::RottenTomatoesAudience,
        Some("rottentomatoes") => Provider::RottenTomatoes,
        Some("metacritic") => Provider::Metacritic,
        _ if kind == "audience" => Provider::PlexAudience,
        _ => Provider::PlexCritic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn projection_preserves_community_scores_and_discards_private_user_state() {
        let body = serde_json::to_vec(&json!({"MediaContainer":{"offset":0,"totalSize":1,"Metadata":[{
            "type":"movie","ratingKey":"42","Guid":[{"id":"tmdb://42"},{"id":"imdb://tt1234567"}],
            "Rating":[{"image":"imdb://image.rating","type":"user","value":1.0},{"image":"imdb://image.rating","type":"audience","value":8.1},{"image":"rottentomatoes://image.rating.ripe","type":"critic","value":0.0},{"image":"rottentomatoes://image.rating.upright","type":"audience","value":9.2}],
            "rating":0.0,"ratingImage":"rottentomatoes://image.rating.ripe","userRating":2.5,"viewCount":7,"lastViewedAt":123_456,"path":"never-leak","token":"never-leak"
        }]}})).expect("Plex fixture");
        let mut output = vec![b'['];
        let result = page(&body, 1, 0).expect("page");

        assert!(result.finished);
        result.append(&body, 1, &mut output, 4096).expect("details");

        output.push(b']');
        let records: Vec<RatingRecord> = serde_json::from_slice(&output).expect("records");

        assert_eq!(records[0].tmdb_id, Some(42));
        assert_eq!(records[0].ratings.len(), 3);
        assert!(records[0].ratings[1].score.abs() < f64::EPSILON);
        assert_eq!(
            records[0].ratings[2].provider,
            Provider::RottenTomatoesAudience
        );
        let text = String::from_utf8(output).expect("UTF-8");

        for secret in ["userRating", "viewCount", "lastViewedAt", "never-leak"] {
            assert!(!text.contains(secret));
        }
    }

    #[test]
    fn details_merge_by_key_preserve_listing_only_scores_and_reject_incomplete_batches() {
        let listing = json!({"MediaContainer":{"Metadata":[
            {"type":"show","ratingKey":"2","Guid":[{"id":"tmdb://42"}],"audienceRating":7.2,"audienceRatingImage":"themoviedb://image.rating"},
            {"type":"show","ratingKey":"11","Guid":[{"id":"tmdb://43"}]}
        ]}});
        let details = json!({"MediaContainer":{"Metadata":[
            {"type":"show","ratingKey":"2","Rating":[{"image":"imdb://image.rating","type":"audience","value":8.1}]},
            {"type":"show","ratingKey":"11"}
        ]}});
        let raw = serde_json::to_vec(&listing).expect("listing");
        let inventory = page(&raw, 2, 0).expect("inventory");
        let mut output = vec![b'['];

        assert_eq!(inventory.metadata_path(), "library/metadata/11,2");
        inventory
            .append(
                &serde_json::to_vec(&details).expect("details"),
                2,
                &mut output,
                4096,
            )
            .expect("full community metadata");
        output.push(b']');
        let rows: Vec<RatingRecord> = serde_json::from_slice(&output).expect("records");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].tmdb_id, Some(42));
        assert_eq!(rows[0].ratings.len(), 2);
        assert_eq!(rows[0].ratings[0].provider, Provider::Imdb);
        assert_eq!(rows[0].ratings[1].provider, Provider::Tmdb);

        for invalid in [
            json!({"MediaContainer":{"Metadata":[]}}),
            json!({"MediaContainer":{"Metadata":[{"type":"show","ratingKey":"2"},{"type":"show","ratingKey":"3"}]}}),
            json!({"MediaContainer":{"Metadata":[{"type":"show","ratingKey":"2"},{"type":"show","ratingKey":"2"}]}}),
        ] {
            assert!(
                page(&raw, 2, 0)
                    .expect("inventory")
                    .append(
                        &serde_json::to_vec(&invalid).expect("invalid batch"),
                        2,
                        &mut vec![b'['],
                        4096
                    )
                    .is_err()
            );
        }

        for key in ["../1", "1?token=secret", "0", "1,2", ""] {
            let invalid = json!({"MediaContainer":{"Metadata":[{"type":"movie","ratingKey":key}]}});

            assert!(page(&serde_json::to_vec(&invalid).expect("invalid key"), 1, 0).is_err());
        }
    }

    #[test]
    fn paging_rejects_stalled_offsets_wrong_kinds_and_oversized_projection() {
        for value in [
            json!({"MediaContainer":{"offset":0,"totalSize":5,"Metadata":[]}}),
            json!({"MediaContainer":{"offset":1,"totalSize":0,"Metadata":[]}}),
            json!({"MediaContainer":{"Metadata":[{"type":"episode"}]}}),
        ] {
            assert!(page(&serde_json::to_vec(&value).expect("fixture"), 1, 0,).is_err());
        }

        let value = json!({"MediaContainer":{"Metadata":[{"type":"movie","ratingKey":"42","rating":8.0,"Guid":[{"id":"tmdb://42"}]}]}});

        assert_eq!(
            page(&serde_json::to_vec(&value).expect("fixture"), 1, 0)
                .expect("page")
                .append(
                    &serde_json::to_vec(&value).expect("fixture"),
                    1,
                    &mut vec![b'['],
                    10
                )
                .err(),
            Some(ApiError::RESPONSE_TOO_LARGE)
        );
    }
}
