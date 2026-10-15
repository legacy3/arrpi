//! Synthetic native fixtures shared by allocation gates and Divan benches.
use arrpi_core::{
    arguments::MediaType,
    library::{LibraryFilter, LibraryPage, paginate, select},
};
use serde_json::json;

pub fn fixture(count: usize, heavy: bool) -> Vec<u8> {
    let rows = (0..count).map(|index| {
        let mut row = json!({
            "id":index+1,"title":format!("Movie {index:06}"),"originalTitle":format!("Original {index:06}"),
            "year":2020,"tmdbId":index+1,"imdbId":"tt1234567","hasFile":true,"genres":["Comedy","Drama"],
            "overview":"A synthetic German library description. ".repeat(16),"runtime":110,"certification":"FSK 12",
            "originalLanguage":{"name":"English"},"ratings":{"imdb":{"value":7.5,"votes":1000}},
            "inCinemas":"2020-01-01","added":"2026-10-01T00:00:00Z"
        });

        if heavy {
            row["movieFile"] = json!({"path":"synthetic-fixture-only","mediaInfo":{"irrelevant":"x".repeat(1024)}});
        }

        row
    }).collect::<Vec<_>>();

    serde_json::to_vec(&rows).expect("synthetic native fixture")
}

pub fn query(limit: usize, narrow: bool, late: bool, count: usize) -> LibraryFilter {
    serde_json::from_value(json!({"limit":limit,"genres":["Comedy"],"offset":if late {count.saturating_sub(limit)} else {0},"query":narrow.then_some("Movie 000000")})).expect("benchmark query")
}

pub fn page<'a>(body: &'a [u8], query: &LibraryFilter, kind: MediaType) -> LibraryPage<'a> {
    let compiled = query.compile().expect("compiled filter");
    let rows = select("source", kind, body, &compiled).expect("valid native snapshot");

    paginate(vec![rows], query).expect("selected page")
}
