//! Deterministic allocation gates run Divan in an isolated child process.
//! Cargo.toml registers this target with harness=false; main owns the gates.
#[path = "support/library.rs"]
mod support;

use arrpi_core::arguments::MediaType;
use divan::{AllocProfiler, Bencher};
use std::fmt;

#[global_allocator]
static ALLOCATOR: AllocProfiler = AllocProfiler::system();

#[derive(Clone, Copy, Debug)]
enum Shape {
    Page,
    Search,
    Late,
    Tv,
}

fn main() {
    if std::env::var_os("ARRPI_ALLOCATION_CHILD").is_some() {
        divan::Divan::default()
            .color(false)
            .sample_count(1)
            .sample_size(1)
            .run_benches();

        return;
    }

    let output =
        std::process::Command::new(std::env::current_exe().expect("allocation test executable"))
            .env("ARRPI_ALLOCATION_CHILD", "1")
            .output()
            .expect("run isolated Divan allocation probe");

    assert!(
        output.status.success(),
        "allocation probe failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = String::from_utf8(output.stdout).expect("Divan UTF-8 report");

    for (shape, max_allocations, max_peak, max_grows, max_growth) in [
        ("Page", 70_000.0, 50_000.0, 0.0, 0.0),
        ("Search", 100_000.0, 50_000.0, 0.0, 0.0),
        ("Late", 70_000.0, 4_000_000.0, 10.0, 2_000_000.0),
        ("Tv", 70_000.0, 50_000.0, 0.0, 0.0),
    ] {
        let section = section(&report, shape);
        let allocated = metric(&section, "alloc:", 0).expect("Divan allocation count");
        let peak = metric(&section, "max alloc:", 1).expect("Divan peak live bytes");
        let grows = metric(&section, "grow:", 0).unwrap_or_default();
        let growth = metric(&section, "grow:", 1).unwrap_or_default();

        assert!(
            allocated < max_allocations,
            "{shape}: allocations {allocated};\n{section}"
        );
        assert!(
            peak < max_peak,
            "{shape}: temporary heap {peak};\n{section}"
        );
        assert!(
            grows <= max_grows,
            "{shape}: growing reallocations {grows};\n{section}"
        );
        assert!(
            growth <= max_growth,
            "{shape}: reallocation growth {growth};\n{section}"
        );
        println!(
            "10k/{shape}: allocations={allocated} peak_bytes={peak} reallocations={grows} growth_bytes={growth}"
        );
    }
}

#[divan::bench(args = [Shape::Page,Shape::Search,Shape::Late,Shape::Tv], sample_count = 1, sample_size = 1)]
fn library_shape(bencher: Bencher, shape: Shape) {
    let mut body = support::fixture(10_000, true);
    let mut kind = MediaType::Movie;

    if matches!(shape, Shape::Tv) {
        let mut records: Vec<serde_json::Value> =
            serde_json::from_slice(&body).expect("movie fixture");

        for record in &mut records {
            record["statistics"] = serde_json::json!({"episodeFileCount":2,"episodeCount":10,"totalEpisodeCount":20,"seasonCount":3});
        }

        body = serde_json::to_vec(&records).expect("series fixture");
        kind = MediaType::Tv;
    }

    let query = support::query(
        20,
        matches!(shape, Shape::Search),
        matches!(shape, Shape::Late),
        10_000,
    );

    bencher.bench_local(|| {
        let page = support::page(&body, &query, kind);
        let expected = if matches!(shape, Shape::Search) {
            1
        } else {
            20
        };

        assert_eq!(
            page.items.len(),
            expected,
            "allocation gate exercises a real page"
        );
        divan::black_box_drop(page);
    });
}

fn section(report: &str, shape: &str) -> String {
    let lines = report.lines().collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|line| first_column(line).split_whitespace().next() == Some(shape))
        .expect("requested allocation case is present");
    let end = lines
        .iter()
        .skip(start + 1)
        .position(|line| {
            first_column(line)
                .split_whitespace()
                .next()
                .is_some_and(|word| matches!(word, "Page" | "Search" | "Late" | "Tv"))
        })
        .map_or(lines.len(), |end| end + start + 1);
    let section = lines[start..end].join("\n");

    for line in section.lines() {
        let cell = first_column(line);

        if cell.ends_with(':') {
            assert!(
                matches!(
                    cell,
                    "max alloc:" | "alloc:" | "dealloc:" | "grow:" | "shrink:"
                ),
                "unsupported Divan metric: {cell}"
            );
        }
    }

    section
}

fn metric(report: &str, name: &str, row: usize) -> Option<f64> {
    let lines = report.lines().collect::<Vec<_>>();
    let position = lines.iter().position(|line| first_column(line) == name)?;
    let cell = first_column(
        lines
            .get(position + row + 1)
            .expect("complete Divan allocation metric"),
    );
    let mut fields = cell.split_whitespace();
    let value = fields
        .next()
        .expect("metric value")
        .parse::<f64>()
        .expect("numeric metric");
    let scale = match fields.next() {
        Some("KB") => 1000.0,
        Some("MB") => 1_000_000.0,
        Some("GB") => 1_000_000_000.0,
        Some("B") | None => 1.0,
        other => panic!("unsupported Divan allocation unit: {other:?}"),
    };

    Some(value * scale)
}

fn first_column(line: &str) -> &str {
    line.trim_start_matches(|character: char| {
        character.is_whitespace() || matches!(character, '│' | '├' | '─' | '╰')
    })
    .split('│')
    .next()
    .expect("metric first column")
    .trim()
}

impl fmt::Display for Shape {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
