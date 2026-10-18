"use client";

import type { UseQueryResult } from "@tanstack/react-query";
import type { LucideIcon } from "lucide-react";

import { useQueries } from "@tanstack/react-query";
import {
  CircleAlert,
  CircleCheck,
  Clock3,
  LoaderCircle,
  RefreshCw,
} from "lucide-react";
import Link from "next/link";

import type { Api, Source, Status } from "@/lib/api";
import type { Components } from "@/lib/generated/contracts";

import { Panel } from "@/components/panel";
import { connectionHealthOptions } from "@/components/settings/connection-status";
import { Button } from "@/components/ui/button";
import { appInfo, fullTime, shortTime } from "@/lib/presentation";
import { cn } from "@/lib/utils";

type Health = Components["schemas"]["ConnectionHealth"];
type HealthQuery = UseQueryResult<Health>;
type Presentation = { icon: LucideIcon; label: string; tone: string };
const good = { icon: CircleCheck, label: "Healthy", tone: "text-emerald-300" };
const checking = {
  icon: LoaderCircle,
  label: "Checking",
  tone: "text-muted-foreground",
};

export function StackHealth({
  api,
  status,
}: Readonly<{ api: Api; status: Status }>) {
  const queries = useQueries({
    queries: status.sources.map((source) => ({
      ...connectionHealthOptions(api, source.name),
      refetchInterval: 60_000,
    })),
  });
  const fetching = queries.some((query) => query.isFetching);
  const healthy = queries.filter(
    (query) => query.data?.status === "healthy" && !query.isError,
  ).length;
  const issues = queries.filter(
    (query) => query.isError || (query.data && query.data.status !== "healthy"),
  ).length;
  const observed = queries
    .map((query) => Math.max(query.dataUpdatedAt, query.errorUpdatedAt))
    .filter((time) => time > 0);
  const time =
    observed.length > 0 ? new Date(Math.min(...observed)).toISOString() : null;
  const presentation = overallState(fetching, issues, status.sources.length);
  const Icon = presentation.icon;

  return (
    <Panel
      title="Stack health"
      detail={
        <Button
          size="icon-sm"
          variant="ghost"
          aria-label="Check service health"
          disabled={fetching}
          onClick={() => {
            for (const query of queries) {
              void query.refetch();
            }
          }}
        >
          <RefreshCw className="size-4" />
        </Button>
      }
    >
      <div className="flex flex-wrap items-center justify-between gap-3 px-5 pb-3">
        <div
          role="status"
          className="flex flex-wrap items-center gap-2 font-medium"
        >
          <Icon
            className={cn(
              "size-4",
              presentation.tone,
              fetching && "animate-spin",
            )}
          />
          {presentation.label}
          <span className="text-xs font-normal text-muted-foreground">
            {healthy}/{status.sources.length} healthy
          </span>
        </div>
        {time && (
          <time
            dateTime={time}
            title={fullTime(time)}
            className="flex items-center gap-1.5 text-xs text-muted-foreground"
          >
            <Clock3 className="size-3.5" />
            Checked {shortTime(time)}
          </time>
        )}
      </div>
      {status.sources.length === 0 && (
        <p className="px-5 pb-4 text-sm">
          <Link href="/settings" className="text-primary underline">
            Add a connection
          </Link>
        </p>
      )}
      <ul className="grid border-t sm:grid-cols-2">
        {queries.map((query, index) => {
          const source = status.sources[index];

          return source ? (
            <HealthRow key={source.name} source={source} query={query} />
          ) : null;
        })}
      </ul>
    </Panel>
  );
}

function HealthRow({
  query,
  source,
}: Readonly<{ source: Source; query: HealthQuery }>) {
  const presentation = sourceState(query);
  const AppIcon = appInfo(source.kind).icon;
  const Icon = presentation.icon;

  return (
    <li className="border-b last:border-b-0">
      <Link
        href={`/settings?entry=${encodeURIComponent(source.name)}`}
        className="flex min-h-11 items-center justify-between gap-2 px-5 py-2 hover:bg-accent/50"
      >
        <span className="flex min-w-0 items-center gap-2 text-sm">
          <AppIcon className="size-4 shrink-0 text-muted-foreground" />
          <span className="truncate">{source.name}</span>
        </span>
        <span
          className={cn(
            "flex shrink-0 items-center gap-1.5 text-xs",
            presentation.tone,
          )}
        >
          <Icon
            className={cn("size-3.5", query.isFetching && "animate-spin")}
          />
          {presentation.label}
        </span>
      </Link>
    </li>
  );
}

function overallState(
  fetching: boolean,
  issues: number,
  total: number,
): Presentation {
  if (fetching) {
    return { ...checking, label: "Checking services" };
  }

  if (!total) {
    return {
      icon: CircleAlert,
      label: "No services configured",
      tone: "text-muted-foreground",
    };
  }

  if (issues) {
    return {
      icon: CircleAlert,
      label: `${issues} ${issues === 1 ? "service needs" : "services need"} attention`,
      tone: "text-amber-300",
    };
  }

  return { ...good, label: "All services healthy" };
}

function sourceState(query: HealthQuery): Presentation {
  if (query.isFetching) {
    return checking;
  }

  if (query.isError) {
    return { icon: CircleAlert, label: "Check failed", tone: "text-amber-300" };
  }

  if (query.data?.status === "healthy") {
    return good;
  }

  const labels: Record<string, string> = {
    attention: `${query.data?.health_issue_count ?? 1} health issues`,
    authentication_failed: "Check API key",
    unavailable: "Unavailable",
    unconfigured: "Not configured",
  };

  return {
    icon: CircleAlert,
    label: labels[query.data?.status ?? "unavailable"] ?? "Unavailable",
    tone: "text-amber-300",
  };
}
