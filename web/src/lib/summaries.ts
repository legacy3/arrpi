import type { Source, SummaryKind } from "@/lib/api";
import type { Components } from "@/lib/generated/contracts";

export const summaryLabels: Record<SummaryKind, string> = {
  library: "Library",
  queue: "Queue",
  status: "Monitoring",
};

export type Summary = Components["schemas"]["Summary"];
export type SummaryRow = Components["schemas"]["SummaryRow"];

type Metric = { label: string; value: string };

export function bytes(value: number) {
  const unit = Math.min(
    4,
    Math.max(0, Math.floor(Math.log2(Math.max(1, value)) / 10)),
  );

  return `${(value / 1024 ** unit).toLocaleString(undefined, { maximumFractionDigits: 1 })} ${["B", "KiB", "MiB", "GiB", "TiB"][unit]}`;
}

export function summaryKinds(sources: Source[]) {
  return (Object.keys(summaryLabels) as SummaryKind[]).filter((kind) =>
    sources.some((source) => source.summaries.includes(kind)),
  );
}

export function summaryMetrics(row: SummaryRow): Metric[] {
  if (row.status !== "ok") {
    return [];
  }

  const data = row.data;

  if ("items" in data) {
    return [
      metric("Items", data.items),
      metric("With files", data.items_with_files),
      metric("Monitored", data.monitored_items),
      metric("Size", data.size_bytes, "bytes"),
    ];
  }

  if ("downloads_observed" in data) {
    return [
      metric("Downloads", data.downloads_observed),
      metric("Queue rows", data.items_observed),
      metric("Observed remaining", data.remaining_bytes_observed, "bytes"),
    ];
  }

  if ("queued_jobs" in data) {
    return [
      metric("Queued jobs", data.queued_jobs),
      metric("Active jobs", data.active_jobs),
      { label: "State", value: data.paused ? "Paused" : "Running" },
      { label: "Speed", value: `${bytes(data.speed_kib_per_second * 1024)}/s` },
      metric(
        "Active remaining",
        data.active_remaining_mebibytes * 1024 ** 2,
        "bytes",
      ),
    ];
  }

  if ("missing_subtitle_requirements" in data) {
    return [
      metric(
        "Episode subtitle requirements",
        data.missing_subtitle_requirements.episodes,
      ),
      metric(
        "Movie subtitle requirements",
        data.missing_subtitle_requirements.movies,
      ),
      metric("Throttled providers", data.throttled_providers),
      metric("Health issues", data.health_issues),
    ];
  }

  if ("removed_downloads" in data) {
    return [
      metric("Removed in 7 days", data.removed_downloads),
      metric("Completed searches", data.searches.completed),
      metric("Failed searches", data.searches.failed),
      metric("Grabs", data.searches.grabbed),
    ];
  }

  return [
    metric("Handled so far", data.cleanup.itemsHandled),
    metric("Deleted so far", data.cleanup.bytesHandled, "bytes"),
    metric("Reclaimable collections", data.collections.reclaimableCount),
    metric(
      data.collections.reclaimable_size_complete
        ? "Reclaimable"
        : "Reclaimable estimate",
      data.collections.activeSizeBytes,
      "bytes",
    ),
  ];
}

export function summaryWarnings(row: SummaryRow) {
  if (row.status !== "ok") {
    return [];
  }

  const { data, freshness } = row;
  const warnings: string[] = [];

  if (!data.complete) {
    warnings.push("Partial data");
  }

  if (freshness.cache === "stale") {
    warnings.push("Stale");
  }

  if ("byte_counts_complete" in data && !data.byte_counts_complete) {
    warnings.push("Incomplete byte counts");
  }

  if ("size_bytes" in data && data.size_bytes === null) {
    warnings.push("Unmeasured library sizes");
  }

  if ("collections" in data && !data.collections.reclaimable_size_complete) {
    warnings.push("Collection sizes incomplete or overlapping");
  }

  return warnings;
}

function metric(
  label: string,
  value: number | null,
  unit: "number" | "bytes" = "number",
): Metric {
  if (value === null) {
    return { label, value: "Unknown" };
  }

  return {
    label,
    value: unit === "bytes" ? bytes(value) : value.toLocaleString(),
  };
}
