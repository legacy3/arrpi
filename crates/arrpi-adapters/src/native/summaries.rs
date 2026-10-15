//! Bounded queue traversal and streaming library counters without media metadata.
use super::Native;
use arrpi_core::{
    error::ApiError,
    source::{LibrarySource, ReadMode, SourceRead},
    summaries::{
        LibrarySummary, QueueConsistency, QueueSummary, SummaryData, SummaryKind, SummarySource,
    },
};
use async_trait::async_trait;
use serde::{
    Deserialize,
    de::{SeqAccess, Visitor},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    time::Duration,
};

#[derive(Debug, Default)]
struct QueueTotals {
    seen: BTreeSet<u64>,
    statuses: BTreeMap<String, usize>,
    downloads: BTreeMap<(String, String), (f64, f64)>,
    unknown_download_rows: usize,
    reported: Option<usize>,
    reason: Option<&'static str>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueuePage {
    page: usize,
    page_size: usize,
    total_records: usize,
    records: arrayvec::ArrayVec<QueueItem, 100>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueueItem {
    id: u64,
    size: f64,
    sizeleft: f64,
    download_id: Option<String>,
    download_client: Option<String>,
    status: String,
}

impl QueueTotals {
    fn add(&mut self, page: QueuePage, expected: usize) -> Result<bool, ApiError> {
        if page.page != expected {
            if page.page > 0 && page.page < expected {
                self.reason = Some("queue_changed");

                return Ok(true);
            }

            return Err(ApiError::INVALID_RESPONSE);
        }

        if page.page_size != 100 {
            return Err(ApiError::INVALID_RESPONSE);
        }

        if self
            .reported
            .is_some_and(|total| total != page.total_records)
        {
            self.reason = Some("queue_changed");
        }

        self.reported = Some(page.total_records);
        let empty = page.records.is_empty();

        for item in page.records {
            self.observe(item)?;
        }

        if empty && self.seen.len() < page.total_records {
            self.reason = Some("queue_changed");
        }

        Ok(empty || self.seen.len() >= page.total_records)
    }

    fn observe(&mut self, item: QueueItem) -> Result<(), ApiError> {
        if !item.size.is_finite()
            || item.size < 0.0
            || !item.sizeleft.is_finite()
            || item.sizeleft < 0.0
        {
            return Err(ApiError::INVALID_RESPONSE);
        }

        if !self.seen.insert(item.id) {
            self.reason = Some("queue_changed");

            return Ok(());
        }

        let status = match item.status.as_str() {
            "downloading" | "paused" | "queued" | "completed" | "failed" | "warning" | "delay" => {
                item.status
            }

            _ => "other".to_owned(),
        };

        *self.statuses.entry(status).or_default() += 1;

        match (item.download_client, item.download_id) {
            (Some(client), Some(id))
                if !client.is_empty()
                    && !id.is_empty()
                    && client.len() <= 256
                    && id.len() <= 256 =>
            {
                self.downloads
                    .entry((client, id))
                    .or_insert((item.size, item.sizeleft));
            }

            _ => self.unknown_download_rows += 1,
        }

        Ok(())
    }
}

impl Native {
    fn queue_page(
        &self,
        totals: &mut QueueTotals,
        body: &[u8],
        page: usize,
    ) -> Result<bool, ApiError> {
        let _lease = self.normalizations.try_acquire()?;

        totals.add(serde_json::from_slice(body)?, page)
    }

    async fn queue_summary(&self, mode: ReadMode) -> Result<SourceRead<QueueSummary>, ApiError> {
        let deadline = self.cache.now() + Duration::from_secs(15);
        let mut totals = QueueTotals::default();
        let mut provenance = Vec::with_capacity(20);
        let mut freshness = None;

        for page in 1..=20 {
            let query = format!(
                "page={page}&pageSize=100&sortKey=added&sortDirection=ascending&includeUnknownSeriesItems=true&includeUnknownMovieItems=true"
            );
            let read =
                match tokio::time::timeout_at(deadline, self.cached("queue", &query, mode)).await {
                    Ok(Ok(read)) => read,

                    Ok(Err(error)) => {
                        if freshness.is_none() {
                            return Err(error);
                        }

                        totals.reason = Some(error.code());
                        break;
                    }

                    Err(_) => {
                        if freshness.is_none() {
                            return Err(ApiError::TIMEOUT);
                        }

                        totals.reason = Some("deadline");
                        break;
                    }
                };
            let observed = self.freshness(&read);

            provenance.push(observed.document(mode == ReadMode::Fresh));

            if freshness
                .as_ref()
                .is_none_or(|prior: &arrpi_core::source::Freshness| {
                    prior.age_seconds < observed.age_seconds
                })
            {
                freshness = Some(observed);
            }

            let stop = match self.queue_page(&mut totals, &read.body, page) {
                Ok(stop) => stop,

                Err(error) => {
                    if page == 1 {
                        return Err(error);
                    }

                    totals.reason = Some(error.code());
                    true
                }
            };

            if stop {
                break;
            }
        }

        finish(totals, provenance, freshness)
    }
}

fn finish(
    mut totals: QueueTotals,
    provenance: Vec<arrpi_core::source::PublicFreshness>,
    freshness: Option<arrpi_core::source::Freshness>,
) -> Result<SourceRead<QueueSummary>, ApiError> {
    if totals
        .reported
        .is_some_and(|total| totals.seen.len() < total)
        && totals.reason.is_none()
    {
        totals.reason = Some("page_limit");
    }

    if totals
        .reported
        .is_some_and(|total| totals.seen.len() > total)
        && totals.reason.is_none()
    {
        totals.reason = Some("queue_changed");
    }

    let complete = totals.reason.is_none() && totals.reported == Some(totals.seen.len());

    let (bytes, remaining) = totals
        .downloads
        .values()
        .fold((0.0_f64, 0.0_f64), |(bytes, remaining), (size, left)| {
            (bytes + size, remaining + left)
        });

    if !bytes.is_finite() || !remaining.is_finite() {
        return Err(ApiError::INVALID_RESPONSE);
    }

    let data = QueueSummary {
        complete,
        consistency: QueueConsistency::NonAtomicPages,
        reason: totals.reason.map(str::to_owned),
        items_observed: totals.seen.len(),
        total_reported: totals.reported,
        downloads_observed: totals.downloads.len(),
        rows_without_download_identity: totals.unknown_download_rows,
        byte_counts_complete: complete && totals.unknown_download_rows == 0,
        byte_counts_approximate: true,
        size_bytes_observed: bytes,
        remaining_bytes_observed: remaining,
        statuses: totals.statuses,
        page_limit: 20,
        pages: provenance,
    };

    Ok(SourceRead {
        data,
        freshness: freshness.ok_or(ApiError::INVALID_RESPONSE)?,
    })
}

#[derive(Debug, Default)]
struct LibraryTotals {
    count: u64,
    available: u64,
    monitored: u64,
    bytes: u64,
    unknown_bytes: u64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LibraryRow {
    #[serde(default)]
    has_file: bool,
    #[serde(default)]
    monitored: bool,
    size_on_disk: Option<u64>,
    statistics: Option<Statistics>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Statistics {
    size_on_disk: Option<u64>,
    #[serde(default)]
    episode_file_count: u64,
}
impl<'de> Deserialize<'de> for LibraryTotals {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Counter;
        impl<'de> Visitor<'de> for Counter {
            type Value = LibraryTotals;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a native library array")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut total = LibraryTotals::default();

                while let Some(row) = seq.next_element::<LibraryRow>()? {
                    total.count += 1;
                    total.available += u64::from(
                        row.has_file
                            || row
                                .statistics
                                .as_ref()
                                .is_some_and(|stats| stats.episode_file_count > 0),
                    );
                    total.monitored += u64::from(row.monitored);

                    if let Some(size) = row
                        .size_on_disk
                        .or_else(|| row.statistics.and_then(|stats| stats.size_on_disk))
                    {
                        total.bytes = total.bytes.checked_add(size).ok_or_else(|| {
                            serde::de::Error::custom("native byte count overflow")
                        })?;
                    } else {
                        total.unknown_bytes += 1;
                    }
                }

                Ok(total)
            }
        }
        deserializer.deserialize_seq(Counter)
    }
}

#[async_trait]
impl SummarySource for Native {
    fn supports(&self, kind: SummaryKind) -> bool {
        matches!(kind, SummaryKind::Queue | SummaryKind::Library)
    }

    async fn summary(
        &self,
        kind: SummaryKind,
        mode: ReadMode,
    ) -> Result<SourceRead<SummaryData>, ApiError> {
        match kind {
            SummaryKind::Queue => {
                let read = self.queue_summary(mode).await?;

                Ok(SourceRead {
                    data: SummaryData::Queue(read.data),
                    freshness: read.freshness,
                })
            }

            SummaryKind::Library => {
                let read = self.snapshot(mode).await?;
                let _lease = self.normalizations.try_acquire()?;
                let total: LibraryTotals = serde_json::from_slice(read.data.as_ref())?;

                Ok(SourceRead {
                    data: SummaryData::Library(LibrarySummary {
                        complete: true,
                        items: total.count,
                        items_with_files: total.available,
                        monitored_items: total.monitored,
                        size_bytes: (total.unknown_bytes == 0).then_some(total.bytes),
                        known_size_bytes: total.bytes,
                        items_without_size: total.unknown_bytes,
                    }),
                    freshness: read.freshness,
                })
            }

            SummaryKind::Status => Err(ApiError::NOT_FOUND),
        }
    }
}
