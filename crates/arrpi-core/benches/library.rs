//! Allocation and timing matrix for minimal native library pagination.
#[path = "../tests/support/library.rs"]
mod support;

use arrpi_core::arguments::MediaType;
use divan::{AllocProfiler, Bencher};
use std::fmt;

#[global_allocator]
static ALLOCATOR: AllocProfiler = AllocProfiler::system();

#[derive(Clone, Copy, Debug)]
struct Case {
    count: usize,
    limit: usize,
    heavy: bool,
    narrow: bool,
    late: bool,
}

fn main() {
    divan::main();
}

fn cases() -> Vec<Case> {
    let mut cases = Vec::with_capacity(48);

    for count in [100, 1000, 10_000] {
        for limit in [20, 500] {
            for (heavy, narrow, late) in [
                (false, false, false),
                (false, false, true),
                (false, true, false),
                (false, true, true),
                (true, false, false),
                (true, false, true),
                (true, true, false),
                (true, true, true),
            ] {
                cases.push(Case {
                    count,
                    limit,
                    heavy,
                    narrow,
                    late,
                });
            }
        }
    }

    cases
}

#[divan::bench(args = cases(), sample_count = 10, sample_size = 1)]
fn normalized_page(bencher: Bencher, case: &Case) {
    let body = support::fixture(case.count, case.heavy);
    let query = support::query(case.limit, case.narrow, case.late, case.count);

    bencher.bench_local(|| divan::black_box_drop(support::page(&body, &query, MediaType::Movie)));
}

#[divan::bench(args = [100, 1000, 10_000], sample_count = 10, sample_size = 1)]
fn serialized_page_20(bencher: Bencher, count: usize) {
    let body = support::fixture(count, true);
    let query = support::query(20, false, false, count);

    bencher.bench_local(|| {
        let page = support::page(&body, &query, MediaType::Movie);
        let response = serde_json::to_vec(&page).expect("serialize public page");

        divan::black_box_drop(response);
    });
}

#[divan::bench(args = [100, 1000, 10_000], sample_count = 10, sample_size = 1)]
fn plex_enriched_page(bencher: Bencher, count: usize) {
    use arrpi_core::library::{RatingIndex, paginate, select_with_ratings};
    let body = support::fixture(count, true);
    let records = (1..=count).map(|id| serde_json::json!({"media_type":"movie","tmdb_id":id,"tvdb_id":null,"imdb_id":null,"ratings":[{"provider":"rottenTomatoes","origin":"plex","score":8.5,"raw_value":8.5,"raw_scale":10.0,"votes":null}]})).collect::<Vec<_>>();
    let mut index = RatingIndex::new();

    index
        .extend(&serde_json::to_vec(&records).expect("ratings fixture"))
        .expect("rating index");
    let query = support::query(20, false, false, count);
    let filter = query.compile().expect("filter");

    bencher.bench_local(|| {
        let rows = select_with_ratings("movies", MediaType::Movie, &body, &filter, &index)
            .expect("selection");

        divan::black_box_drop(paginate(vec![rows], &query).expect("page"));
    });
}

impl fmt::Display for Case {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "n{}-page{}-{}-{}-{}",
            self.count,
            self.limit,
            if self.heavy { "heavy" } else { "lean" },
            if self.narrow { "narrow" } else { "broad" },
            if self.late { "late" } else { "early" }
        )
    }
}
