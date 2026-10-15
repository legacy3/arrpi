//! Configured descriptors and typed calendar response serialization.

use arrpi_core::{
    contracts::{Calendar, CalendarEvent, EventKind, SourceAvailability, SourceStatus},
    policy::Limits,
    registry::{Registry, ToolId},
};
use divan::{AllocProfiler, Bencher};

#[global_allocator]
static ALLOCATOR: AllocProfiler = AllocProfiler::system();

fn main() {
    divan::main();
}

#[divan::bench]
fn registry_construction(bencher: Bencher) {
    bencher.bench_local(|| divan::black_box_drop(Registry::new(Limits::default())));
}

#[divan::bench]
fn configured_descriptor_clone(bencher: Bencher) {
    let registry = Registry::new(Limits::default());

    bencher.bench_local(|| {
        divan::black_box_drop(registry.definition(ToolId::SearchMedia).clone());
    });
}

#[divan::bench]
fn complete_tool_listing(bencher: Bencher) {
    let registry = Registry::new(Limits::default());

    bencher.bench_local(|| {
        let definitions: Vec<_> = ToolId::ALL
            .iter()
            .map(|id| registry.definition(*id).clone())
            .collect();

        divan::black_box_drop(definitions);
    });
}

#[divan::bench(args = [100, 1000, 10_000], sample_count = 20, sample_size = 1)]
fn typed_calendar_response(bencher: Bencher, count: usize) {
    let calendar = calendar(count);

    bencher.bench_local(|| {
        let encoded = serde_json::to_vec(&calendar).expect("typed public calendar serialization");

        divan::black_box_drop(encoded);
    });
}

fn calendar(count: usize) -> Calendar {
    let events = (0..count)
        .map(|index| CalendarEvent {
            source: "sonarr".to_owned(),
            id: u64::try_from(index).expect("bounded fixture index"),
            title: "Grüße aus dem Kalender".to_owned(),
            kind: EventKind::Episode,
            date: "2026-10-15T18:00:00Z".to_owned(),
            series: Some("A series with a longer title".to_owned()),
            season: Some(1),
            episode: Some(2),
            tmdb_id: Some(42),
            imdb_id: Some("tt1234567".to_owned()),
            tvdb_id: Some(7),
            has_file: false,
        })
        .collect();

    Calendar {
        events,
        sources: vec![SourceStatus {
            source: "sonarr".to_owned(),
            status: SourceAvailability::Ok,
            cache: Some(arrpi_core::contracts::CacheState::Hit),
            age_seconds: Some(3),
        }],
    }
}
