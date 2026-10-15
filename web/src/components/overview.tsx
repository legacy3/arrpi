import {
  Box,
  Captions,
  ChevronDown,
  CircleAlert,
  Download,
  Film,
  Gauge,
  Layers,
  Plug,
  Timer,
  Trash2,
  Tv,
} from "lucide-react";

import type { Api, Source, Status } from "@/lib/api";

import { OperationalSummary } from "@/components/operational-summary";
import { Panel } from "@/components/panel";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { summaryKinds, summaryLabels } from "@/lib/summaries";
import { cn } from "@/lib/utils";

export function Overview({
  api,
  status,
}: Readonly<{ api: Api; status: Status }>) {
  const totals = status.sources.reduce(
    (total, source) => ({
      failures: total.failures + source.counters.upstream_failures,
      hits: total.hits + source.counters.hits,
      misses: total.misses + source.counters.misses,
      stale: total.stale + source.counters.stale,
    }),
    { failures: 0, hits: 0, misses: 0, stale: 0 },
  );
  const ratio =
    totals.hits + totals.misses
      ? `${Math.round((totals.hits / (totals.hits + totals.misses)) * 100)}%`
      : "—";
  const metrics = [
    {
      color: "text-primary",
      icon: Gauge,
      title: "Cache hit rate",
      value: ratio,
    },
    {
      icon: Layers,
      title: "Cache misses",
      value: totals.misses.toLocaleString(),
    },
    {
      color: totals.stale ? "text-amber-300" : "",
      icon: Timer,
      title: "Stale responses",
      value: totals.stale.toLocaleString(),
    },
    {
      color: totals.failures ? "text-destructive" : "",
      icon: CircleAlert,
      title: "Upstream failures",
      value: totals.failures.toLocaleString(),
    },
  ];
  const groups = [
    {
      rows: [
        ["Fresh for", `${status.cache.fresh_seconds}s`],
        ["Stale window", `${status.cache.stale_seconds}s`],
        ["Retry backoff", `${status.cache.retry_seconds}s`],
        ["Entries per source", status.cache.max_entries],
      ],
      title: "Caching",
    },
    {
      rows: [
        ["Fetches per source", status.cache.max_upstream_requests],
        ["Upstream timeout", `${status.cache.timeout_seconds}s`],
        ["Response limit", bodySize(status.cache.max_body_bytes)],
        ["Retained body budget", bodySize(status.cache.max_retained_bytes)],
        ["Concurrent normalizations", status.cache.max_normalizations],
        ["API admission", status.cache.max_requests],
      ],
      title: "Requests",
    },
    {
      rows: [
        ["Accepted connections", status.cache.max_connections],
        ["Idle deadline", `${status.cache.connection_idle_seconds}s`],
        ["Maximum lifetime", `${status.cache.connection_lifetime_seconds}s`],
      ],
      title: "Connections",
    },
  ];

  return (
    <>
      <Card className="gap-0 overflow-hidden py-0">
        <dl className="grid grid-cols-2 gap-px bg-border lg:grid-cols-4">
          {metrics.map(({ color, icon: Component, title, value }) => (
            <div key={title} className="space-y-2 bg-card px-4 py-4 sm:px-5">
              <dt className="flex items-center gap-2 text-xs text-muted-foreground">
                <Component className="size-3.5 shrink-0" />
                {title}
              </dt>
              <dd
                className={cn(
                  "text-xl font-semibold tracking-tight tabular-nums",
                  color,
                )}
              >
                {value}
              </dd>
            </div>
          ))}
        </dl>
      </Card>
      <Panel
        title="Sources"
        detail={
          <Badge variant="secondary">
            {status.sources.length} source
            {status.sources.length === 1 ? "" : "s"}
          </Badge>
        }
      >
        <Table>
          <TableHeader>
            <TableRow className="bg-muted/20 hover:bg-muted/20">
              <TableHead className="pl-5 text-xs text-muted-foreground">
                Source
              </TableHead>
              <TableHead className="text-xs text-muted-foreground">
                Access
              </TableHead>
              <TableHead className="hidden text-right text-xs text-muted-foreground sm:table-cell">
                Hits
              </TableHead>
              <TableHead className="pr-5 text-right text-xs text-muted-foreground">
                Misses
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {status.sources.map((source) => (
              <SourceRow key={source.name} source={source} />
            ))}
          </TableBody>
        </Table>
      </Panel>
      {summaryKinds(status.sources).map((kind) => (
        <OperationalSummary key={kind} api={api} kind={kind} />
      ))}
      <Card className="gap-0 py-0">
        <details className="group">
          <summary className="flex cursor-pointer list-none items-center justify-between px-5 py-4 text-sm font-semibold focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-ring [&::-webkit-details-marker]:hidden">
            Runtime policy
            <ChevronDown className="size-4 text-muted-foreground transition-transform group-open:rotate-180" />
          </summary>
          <div className="grid gap-6 border-t px-5 py-5 lg:grid-cols-3">
            {groups.map((group) => (
              <div key={group.title}>
                <h3 className="mb-3 text-sm font-semibold text-foreground">
                  {group.title}
                </h3>
                <dl className="space-y-3">
                  {group.rows.map(([name, value]) => (
                    <div
                      className="flex items-baseline justify-between gap-4 text-xs"
                      key={name}
                    >
                      <dt className="text-muted-foreground">{name}</dt>
                      <dd className="font-mono tabular-nums">{value}</dd>
                    </div>
                  ))}
                </dl>
              </div>
            ))}
          </div>
        </details>
      </Card>
    </>
  );
}

const sourceApps = {
  bazarr: { icon: Captions, label: "Bazarr", version: null },
  cleanuparr: { icon: Trash2, label: "Cleanuparr", version: null },
  maintainerr: { icon: Trash2, label: "Maintainerr", version: null },
  radarr: { icon: Film, label: "Radarr", version: "API v3" },
  sabnzbd: { icon: Download, label: "SABnzbd", version: null },
  seerr: { icon: Plug, label: "Seerr", version: "API v1" },
  sonarr: { icon: Tv, label: "Sonarr", version: "API v3" },
};

function bodySize(bytes: number) {
  return bytes < 1_048_576
    ? `${bytes.toLocaleString()} B`
    : `${(bytes / 1_048_576).toLocaleString(undefined, { maximumFractionDigits: 1 })} MiB`;
}

function SourceRow({ source }: Readonly<{ source: Source }>) {
  const app = Object.hasOwn(sourceApps, source.kind)
    ? sourceApps[source.kind as keyof typeof sourceApps]
    : { icon: Box, label: source.kind, version: null };
  const Component = app.icon;

  return (
    <TableRow>
      <TableCell className="py-4 pl-5">
        <div className="flex items-center gap-3">
          <span
            className={cn(
              "flex size-9 shrink-0 items-center justify-center rounded-lg border",
              source.kind === "sonarr"
                ? "border-emerald-400/15 bg-emerald-400/10 text-emerald-300"
                : "border-primary/20 bg-primary/10 text-primary",
            )}
          >
            <Component className="size-4" />
          </span>
          <div className="space-y-0.5">
            <div className="font-medium">{source.name}</div>
            <div className="text-xs text-muted-foreground">
              {app.label}
              {app.version && (
                <span className="ml-2 text-muted-foreground/60">
                  {app.version}
                </span>
              )}
            </div>
          </div>
        </div>
      </TableCell>
      <TableCell>
        <div className="flex flex-wrap items-center gap-2">
          {source.calendar && (
            <Badge
              variant="outline"
              className="border-emerald-400/20 text-emerald-300"
            >
              Calendar
            </Badge>
          )}
          {source.kind === "seerr" && (
            <Badge variant="outline">Account broker</Badge>
          )}
          {source.summaries.map((kind) => (
            <Badge key={kind} variant="outline">
              {summaryLabels[kind]}
            </Badge>
          ))}
          {source.endpoints.length > 0 && (
            <span className="hidden text-xs text-muted-foreground sm:inline">
              {source.endpoints.length} native endpoints
            </span>
          )}
        </div>
      </TableCell>
      <TableCell className="hidden text-right font-mono text-xs text-muted-foreground sm:table-cell">
        {source.counters.hits.toLocaleString()}
      </TableCell>
      <TableCell className="pr-5 text-right font-mono text-xs">
        {source.counters.misses.toLocaleString()}
      </TableCell>
    </TableRow>
  );
}
