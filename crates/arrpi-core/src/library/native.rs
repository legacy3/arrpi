//! Borrowed, tolerant native fields; unknown media objects are never allocated.
use super::{Availability, FileMetadata, FileQuality, LibraryItem, Rating};
use crate::ratings::{self, Origin, Provider};
use crate::{arguments::MediaType, error::ApiError};
use serde::{
    Deserialize, Deserializer,
    de::{self, IgnoredAny, MapAccess, SeqAccess, Visitor},
};
use serde_json::value::RawValue;
use std::{borrow::Cow, fmt};

// These object decoders preserve JSON's last-key-wins behavior and reject arrays.
macro_rules! raw_object {
    ($visibility:vis struct $name:ident, $visitor:ident { $($field:ident => $key:literal),* $(,)? }) => {
        #[derive(Debug, Default)]
        $visibility struct $name<'a> {
            $($field: Option<&'a RawValue>,)*
        }

        #[derive(Debug)]
        struct $visitor;

        impl<'de> Deserialize<'de> for $name<'de> {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                deserializer.deserialize_map($visitor)
            }
        }

        impl<'de> Visitor<'de> for $visitor {
            type Value = $name<'de>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a native metadata object")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut object = $name::default();

                while let Some(Text(name)) = map.next_key::<Text<'de>>()? {
                    match name.as_ref() {
                        $($key => object.$field = Some(map.next_value::<&RawValue>()?),)*
                        _ => { let _: IgnoredAny = map.next_value()?; }
                    }
                }

                Ok(object)
            }
        }
    };
}

raw_object! {
    pub(super) struct Native, NativeVisitor {
        id => "id", title => "title", original_title => "originalTitle", year => "year",
        tmdb_id => "tmdbId", tvdb_id => "tvdbId", imdb_id => "imdbId", overview => "overview",
        genres => "genres", ratings => "ratings", runtime => "runtime", certification => "certification",
        original_language => "originalLanguage", status => "status", studio => "studio", network => "network",
        added => "added", in_cinemas => "inCinemas", first_aired => "firstAired",
        digital_release => "digitalRelease", physical_release => "physicalRelease",
        monitored => "monitored", has_file => "hasFile", statistics => "statistics",
        movie_file => "movieFile",
    }
}

raw_object! {
    struct Statistics, StatisticsVisitor {
        episode_files => "episodeFileCount", monitored_episodes => "episodeCount",
        total_episodes => "totalEpisodeCount", seasons => "seasonCount",
    }
}

raw_object! {
    struct MovieFile, MovieFileVisitor {
        quality => "quality", media_info => "mediaInfo",
    }
}
raw_object! {
    struct MediaInfo, MediaInfoVisitor {
        audio_languages => "audioLanguages", subtitles => "subtitles", video_codec => "videoCodec",
    }
}
raw_object! { struct QualityModel, QualityModelVisitor { quality => "quality" } }
raw_object! {
    struct NativeQuality, QualityVisitor {
        name => "name", resolution => "resolution", source => "source",
    }
}

raw_object! { struct Language, LanguageVisitor { name => "name" } }
raw_object! { struct NativeRating, RatingVisitor { value => "value", votes => "votes" } }

raw_object! {
    struct NativeRatings, RatingsVisitor {
        value => "value", votes => "votes", imdb => "imdb", tmdb => "tmdb", trakt => "trakt",
        sonarr => "sonarr", metacritic => "metacritic", rotten_tomatoes => "rottenTomatoes",
    }
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Text<'a>(Cow<'a, str>);

#[derive(Debug)]
struct TextVisitor;

#[derive(Debug)]
struct GenresVisitor;

impl<'a> Native<'a> {
    pub(super) fn decode(raw: &'a RawValue) -> Result<Self, ApiError> {
        serde_json::from_str(raw.get()).map_err(|error| {
            tracing::debug!(
                event = "native_library_record",
                category = std::any::type_name_of_val(&error)
            );
            ApiError::INVALID_RESPONSE
        })
    }

    pub(super) fn project(
        self,
        source: &'a str,
        kind: MediaType,
    ) -> Result<LibraryItem<'a>, ApiError> {
        let id = positive(self.id).ok_or(ApiError::INVALID_RESPONSE)?;
        let title = text(self.title, 1024).ok_or(ApiError::INVALID_RESPONSE)?;
        let ratings = ratings(self.ratings);
        let origin = if kind == MediaType::Movie {
            Origin::Radarr
        } else {
            Origin::Sonarr
        };
        let mut ratings = ratings;

        for rating in &mut ratings {
            rating.origin = origin;
        }

        let community = ratings::community(&ratings);
        let rating = community.map(|rating| rating.score);
        let rating_provider = community.map(|rating| rating.provider);
        let statistics: Statistics<'_> = parse(self.statistics).unwrap_or_default();
        let files = parse::<u64>(statistics.episode_files);
        let available = if kind == MediaType::Movie {
            parse(self.has_file).unwrap_or(false)
        } else {
            files.unwrap_or(0) > 0
        };
        let original_language: Option<Language<'_>> = parse(self.original_language);

        Ok(LibraryItem {
            source,
            id,
            media_type: kind,
            title,
            original_title: text(self.original_title, 1024),
            year: parse(self.year),
            tmdb_id: positive(self.tmdb_id),
            tvdb_id: positive(self.tvdb_id),
            imdb_id: text(self.imdb_id, 32),
            overview: text(self.overview, 8000),
            genres: strings(self.genres),
            rating,
            rating_provider,
            ratings,
            runtime_minutes: parse(self.runtime),
            certification: text(self.certification, 64),
            original_language: original_language.and_then(|language| text(language.name, 256)),
            status: text(self.status, 64),
            studio: text(self.studio, 256),
            network: text(self.network, 256),
            added_at: text(self.added, 64),
            release_date: text(
                if kind == MediaType::Movie {
                    self.in_cinemas
                } else {
                    self.first_aired
                },
                64,
            ),
            digital_release_date: text(self.digital_release, 64),
            physical_release_date: text(self.physical_release, 64),
            monitored: parse(self.monitored),
            available,
            availability: availability(kind, &statistics, available),
            file: (kind == MediaType::Movie)
                .then(|| file(self.movie_file))
                .flatten(),
        })
    }
}

fn file(raw: Option<&RawValue>) -> Option<FileMetadata<'_>> {
    let file: MovieFile<'_> = parse(raw)?;
    let info: MediaInfo<'_> = parse(file.media_info).unwrap_or_default();
    let quality_model: QualityModel<'_> = parse(file.quality).unwrap_or_default();
    let quality: NativeQuality<'_> = parse(quality_model.quality).unwrap_or_default();
    let quality = FileQuality {
        name: text(quality.name, 128),
        resolution: positive(quality.resolution).filter(|resolution| *resolution <= 8192),
        source: text(quality.source, 64),
    };
    let quality =
        (quality.name.is_some() || quality.resolution.is_some() || quality.source.is_some())
            .then_some(quality);
    let result = FileMetadata {
        quality,
        audio_languages: text(info.audio_languages, 1024),
        subtitle_languages: text(info.subtitles, 1024),
        video_codec: text(info.video_codec, 64),
    };

    (result.quality.is_some()
        || result.audio_languages.is_some()
        || result.subtitle_languages.is_some()
        || result.video_codec.is_some())
    .then_some(result)
}

fn parse<'a, T>(raw: Option<&'a RawValue>) -> Option<T>
where
    T: Deserialize<'a>,
{
    raw.and_then(|raw| serde_json::from_str(raw.get()).ok())
}

fn positive(raw: Option<&RawValue>) -> Option<u64> {
    parse(raw).filter(|id| *id > 0)
}

fn text(raw: Option<&RawValue>, maximum: usize) -> Option<Cow<'_, str>> {
    parse::<Text<'_>>(raw)
        .map(|text| text.0)
        .filter(|text| text.len() <= maximum)
}

fn strings(raw: Option<&RawValue>) -> Vec<Cow<'_, str>> {
    raw.and_then(|raw| {
        serde_json::Deserializer::from_str(raw.get())
            .deserialize_seq(GenresVisitor)
            .ok()
    })
    .unwrap_or_default()
}

fn ratings(raw: Option<&RawValue>) -> Vec<Rating> {
    let values: NativeRatings<'_> = parse(raw).unwrap_or_default();

    if values.value.is_some() {
        return rating("sonarr", raw).into_iter().collect();
    }

    let providers = [
        ("imdb", values.imdb),
        ("metacritic", values.metacritic),
        ("rottenTomatoes", values.rotten_tomatoes),
        ("sonarr", values.sonarr),
        ("tmdb", values.tmdb),
        ("trakt", values.trakt),
    ];
    let mut ratings = Vec::with_capacity(
        providers
            .iter()
            .filter(|(_, value)| value.is_some())
            .count(),
    );

    for (name, value) in providers {
        if let Some(rating) = rating(name, value) {
            ratings.push(rating);
        }
    }

    ratings
}

fn rating(name: &str, raw: Option<&RawValue>) -> Option<Rating> {
    let (provider, scale) = match name {
        "imdb" => (Provider::Imdb, 10.0),
        "tmdb" => (Provider::Tmdb, 10.0),
        "trakt" => (Provider::Trakt, 10.0),
        "sonarr" => (Provider::Sonarr, 10.0),
        "rottenTomatoes" => (Provider::RottenTomatoes, 100.0),
        "metacritic" => (Provider::Metacritic, 100.0),
        _ => return None,
    };
    let value: NativeRating<'_> = parse(raw)?;
    let raw_value: f64 = parse(value.value)
        .filter(|value: &f64| value.is_finite() && (0.0..=scale).contains(value))?;
    let votes = parse(value.votes);

    if (raw_value <= 0.0 || votes == Some(0)) && scale < 100.0 {
        return None;
    }

    Rating::new(provider, Origin::Radarr, raw_value, scale, votes)
}

fn availability(kind: MediaType, counts: &Statistics<'_>, available: bool) -> Availability {
    if kind == MediaType::Movie {
        return Availability::Movie {
            status: if available { "available" } else { "missing" },
        };
    }

    let files = parse::<u64>(counts.episode_files);
    let episodes = parse::<u64>(counts.monitored_episodes);
    let status = match (files, episodes) {
        (Some(files), Some(episodes)) if episodes > 0 && files >= episodes => "monitored_available",
        (Some(files), _) if files > 0 => "partial",
        _ => "missing",
    };

    Availability::Series {
        status,
        coverage_scope: "monitored_aired_episodes",
        episode_files: files,
        monitored_episodes: episodes,
        total_episodes: parse(counts.total_episodes),
        seasons: parse(counts.seasons),
    }
}

impl<'de> Deserialize<'de> for Text<'de> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(TextVisitor)
    }
}

impl<'de> Visitor<'de> for TextVisitor {
    type Value = Text<'de>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a native text string")
    }

    fn visit_borrowed_str<E>(self, v: &'de str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Text(Cow::Borrowed(v)))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Text(Cow::Owned(v.to_owned())))
    }

    fn visit_string<E>(self, v: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Text(Cow::Owned(v)))
    }
}

impl<'de> Visitor<'de> for GenresVisitor {
    type Value = Vec<Cow<'de, str>>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("an array of native genre names")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut genres = Vec::with_capacity(seq.size_hint().unwrap_or(4).min(64));
        let mut count = 0;

        while let Some(raw) = seq.next_element::<&RawValue>()? {
            if count < 64
                && let Some(genre) = text(Some(raw), 256)
            {
                genres.push(genre);
            }

            count += 1;
        }

        Ok(genres)
    }
}
