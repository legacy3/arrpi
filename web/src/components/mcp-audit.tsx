"use client";

import { useDebounce, useResetState } from "ahooks";
import { Activity, ChevronDown, RefreshCw } from "lucide-react";
import { parseAsInteger, parseAsString, useQueryStates } from "nuqs";
import { useMemo, useState } from "react";

import type { Api } from "@/lib/api";
import type { Components } from "@/lib/generated/contracts";

import {
  CommandProgress,
  type CommandTarget,
} from "@/components/command-progress";
import { Notice } from "@/components/notice";
import { Panel } from "@/components/panel";
import { SearchField } from "@/components/search-field";
import { SearchableChoice } from "@/components/searchable-choice";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  errorText,
  fullTime,
  outcomeLabel,
  shortTime,
} from "@/lib/presentation";
import { useApiQuery } from "@/lib/use-api-query";
type Target = Components["schemas"]["AuditTarget"];
type Tool = Components["schemas"]["ToolVisibility"];

export function McpAudit({
  api,
  client,
  pageLimit,
  tools,
}: Readonly<{ api: Api; client: string; pageLimit: number; tools: Tool[] }>) {
  const [filters, setFilters] = useQueryStates({
    action: parseAsString.withDefault("*"),
    days: parseAsInteger.withDefault(0),
    history: parseAsString.withDefault(""),
    offset: parseAsInteger.withDefault(0),
    outcome: parseAsString.withDefault("*"),
  });
  const [progress, setProgress, closeProgress] =
    useResetState<CommandTarget | null>(null);
  const search = useDebounce(filters.history, { wait: 200 });
  const [now] = useState(() => Date.now());
  const since = useMemo(
    () =>
      filters.days > 0
        ? new Date(now - Math.min(365, filters.days) * 86_400_000).toISOString()
        : null,
    [filters.days, now],
  );
  const offset = Math.max(0, filters.offset);
  const limit = Math.min(10, pageLimit);
  const params = new URLSearchParams({
    client,
    limit: String(limit),
    offset: String(offset),
  });

  if (search) {
    params.set("query", search);
  }

  if (filters.action !== "*") {
    params.set("tool", filters.action);
  }

  if (filters.outcome !== "*") {
    params.set("outcome", filters.outcome);
  }

  if (since) {
    params.set("since", since);
  }

  const query = useApiQuery<Components["schemas"]["AuditPage"]>(
    api,
    `/v1/config/mcp/audit?${params}`,
    { enabled: Boolean(client) },
  );
  const data = query.data?.data;
  const countLabel = filteredCount(
    data?.total,
    Boolean(
      filters.history ||
      filters.action !== "*" ||
      filters.outcome !== "*" ||
      filters.days,
    ),
  );
  const filtered = Boolean(
    filters.history ||
    filters.action !== "*" ||
    filters.outcome !== "*" ||
    filters.days,
  );

  return (
    <Panel
      title="Write history"
      detail={
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label="Refresh write history"
          disabled={query.isFetching}
          onClick={() => query.refresh()}
        >
          <RefreshCw className="size-4" />
        </Button>
      }
    >
      <div className="grid gap-2 px-5 pb-4 sm:grid-cols-2 lg:grid-cols-4">
        <SearchField
          label="Search history"
          value={filters.history}
          onChange={(history) => void setFilters({ history, offset: 0 })}
        />
        <SearchableChoice
          label="Action"
          value={filters.action}
          onChange={(action) => void setFilters({ action, offset: 0 })}
          options={[
            { label: "All actions", value: "*" },
            ...tools.map((tool) => ({ label: tool.title, value: tool.name })),
          ]}
        />
        <SearchableChoice
          label="Outcome"
          value={filters.outcome}
          onChange={(outcome) => void setFilters({ offset: 0, outcome })}
          options={[
            { label: "All outcomes", value: "*" },
            ...(
              [
                "queued",
                "started",
                "completed",
                "approved",
                "pending",
                "failed",
                "unknown_outcome",
                "invalid_arguments",
                "ok",
              ] as const
            ).map((value) => ({ label: outcomeLabel(value), value })),
          ]}
        />
        <SearchableChoice
          label="Time range"
          value={String(filters.days)}
          onChange={(days) =>
            void setFilters({ days: Number(days), offset: 0 })
          }
          options={[
            { label: "All time", value: "0" },
            { label: "Past 24 hours", value: "1" },
            { label: "Past 7 days", value: "7" },
            { label: "Past 30 days", value: "30" },
          ]}
        />
      </div>
      {query.error ? (
        <div className="px-5 pb-4">
          <Notice>{query.error.message}</Notice>
        </div>
      ) : (
        <>
          <Table aria-label="Write history">
            <TableHeader>
              <TableRow>
                <TableHead className="hidden w-36 pl-5 md:table-cell">
                  When
                </TableHead>
                <TableHead className="pl-5 md:pl-2">Action and media</TableHead>
                <TableHead className="pr-5 text-right">Outcome</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {data?.items.map((row) => {
                const label =
                  tools.find((tool) => tool.name === row.tool)?.title ??
                  row.tool;
                const title = targetTitle(row.target);

                return (
                  <TableRow key={row.id}>
                    <TableCell className="hidden pl-5 align-top text-xs text-muted-foreground md:table-cell">
                      <time
                        dateTime={row.created_at}
                        title={fullTime(row.created_at)}
                      >
                        {shortTime(row.created_at)}
                      </time>
                    </TableCell>
                    <TableCell className="w-full max-w-0 whitespace-normal py-3 pl-5 align-top md:pl-2">
                      <div className="text-xs text-muted-foreground">
                        {label}
                      </div>
                      <div className="mt-0.5 break-words font-medium">
                        {row.target.tmdb_id && row.target.media_type ? (
                          <a
                            href={`https://www.themoviedb.org/${row.target.media_type}/${row.target.tmdb_id}`}
                            target="_blank"
                            rel="noreferrer"
                            className="hover:text-primary hover:underline"
                          >
                            {title}
                          </a>
                        ) : (
                          title
                        )}
                      </div>
                      <p className="mt-1 break-words text-xs text-muted-foreground">
                        {targetScope(row.target)}
                      </p>
                      <time
                        className="mt-1 block text-xs text-muted-foreground md:hidden"
                        dateTime={row.created_at}
                        title={fullTime(row.created_at)}
                      >
                        {shortTime(row.created_at)}
                      </time>
                      <details className="mt-1 text-xs text-muted-foreground">
                        <summary className="inline-flex cursor-pointer items-center gap-1">
                          <ChevronDown className="size-3" />
                          Details
                        </summary>
                        <p className="mt-1">{errorText(row.outcome)}</p>
                        <code className="mt-1 block break-all">
                          {JSON.stringify(row.target)}
                        </code>
                      </details>
                    </TableCell>
                    <TableCell className="w-28 whitespace-normal pr-5 text-right align-top">
                      <Badge
                        variant="outline"
                        title={errorText(row.outcome)}
                        className="h-auto min-h-5 max-w-full whitespace-normal text-left text-xs"
                      >
                        {outcomeLabel(row.outcome)}
                      </Badge>
                      {Boolean(row.target.command_id && row.target.source) && (
                        <Button
                          variant="ghost"
                          size="sm"
                          className="mt-1 h-8 px-1 text-xs"
                          aria-label={`Check progress for ${title}`}
                          onClick={() =>
                            setProgress({
                              command_id: row.target.command_id ?? 0,
                              source: row.target.source ?? "",
                              title,
                            })
                          }
                        >
                          <Activity className="size-3.5" />
                          Progress
                        </Button>
                      )}
                    </TableCell>
                  </TableRow>
                );
              })}
              {data?.items.length === 0 && (
                <TableRow>
                  <TableCell
                    colSpan={3}
                    className="px-5 py-6 text-sm text-muted-foreground"
                  >
                    {filtered ? "No matching history." : "No writes yet."}
                    {filtered && (
                      <Button
                        variant="link"
                        size="sm"
                        onClick={() =>
                          void setFilters({
                            action: "*",
                            days: 0,
                            history: "",
                            offset: 0,
                            outcome: "*",
                          })
                        }
                      >
                        Clear filters
                      </Button>
                    )}
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
          <div className="flex items-center justify-between gap-3 px-5 py-3">
            <span className="text-xs text-muted-foreground">{countLabel}</span>
            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                disabled={offset === 0 || query.isFetching}
                onClick={() =>
                  void setFilters({ offset: Math.max(0, offset - limit) })
                }
              >
                Previous
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={data?.next_offset == null || query.isFetching}
                onClick={() =>
                  void setFilters({ offset: data?.next_offset ?? 0 })
                }
              >
                Next
              </Button>
            </div>
          </div>
        </>
      )}
      <CommandProgress api={api} target={progress} onClose={closeProgress} />
    </Panel>
  );
}

function filteredCount(total: number | undefined, filtered: boolean) {
  if (total === undefined) {
    return "Loading...";
  }

  const noun = filtered ? "match" : "entry";
  const plural = filtered ? "matches" : "entries";

  return `${total} ${total === 1 ? noun : plural}`;
}

function targetScope(target: Target) {
  const seasons =
    target.seasons ?? (target.season == null ? [] : [target.season]);
  let missing: string | null = null;

  if (target.missing_only !== undefined && target.missing_only !== null) {
    missing = target.missing_only ? "Missing only" : "Includes upgrades";
  }

  return [
    target.source,
    seasons.map((season) => `S${String(season).padStart(2, "0")}`).join(", "),
    target.episode_ids?.length ? `${target.episode_ids.length} episodes` : null,
    missing,
  ]
    .filter(Boolean)
    .join(", ");
}

function targetTitle(target: Target) {
  if (target.title) {
    return target.title;
  }

  if (target.item_id) {
    return `${target.source ?? "Library"} item #${target.item_id}`;
  }

  if (target.tmdb_id) {
    return `${target.media_type === "tv" ? "TV show" : "Movie"} (TMDB ${target.tmdb_id})`;
  }

  if (target.issue_id) {
    return `Issue #${target.issue_id}`;
  }

  if (target.request_id) {
    return `Request #${target.request_id}`;
  }

  return "Account action";
}
