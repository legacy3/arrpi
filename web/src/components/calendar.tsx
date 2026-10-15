"use client";

import {
  CalendarDays,
  ChevronLeft,
  ChevronRight,
  Clock3,
  RotateCw,
  Search,
} from "lucide-react";
import { useState } from "react";

import { CalendarEvents } from "@/components/calendar-events";
import { Choice } from "@/components/choice";
import { EmptyState } from "@/components/empty-state";
import { Notice } from "@/components/notice";
import { Panel } from "@/components/panel";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  type Api,
  type Calendar as CalendarData,
  currentMonth,
  dateRange,
  HttpError,
  isMonth,
  type Status,
} from "@/lib/api";
import { useApiQuery } from "@/lib/use-api-query";
import { useElapsed } from "@/lib/use-elapsed";
import { cn } from "@/lib/utils";

export function Calendar({
  api,
  status,
}: Readonly<{ api: Api; status: Status }>) {
  const [month, setMonth] = useState(currentMonth);
  const [source, setSource] = useState("");
  const [search, setSearch] = useState("");
  const [shown, setShown] = useState(100);
  const selected = isMonth(month);
  const dates = dateRange(selected ? month : "2000-01");
  const query = useApiQuery<CalendarData>(
    api,
    `/v1/calendar?${new URLSearchParams(dates)}`,
    { enabled: selected },
  );
  const data = query.data?.data ?? failedCalendar(query.error);
  const elapsed = useElapsed(query.data?.receivedAt);
  const events = (data?.events ?? []).filter(
    (event) =>
      (!source || event.source === source) &&
      `${event.series ?? ""} ${event.title}`
        .toLowerCase()
        .includes(search.toLowerCase()),
  );
  const error = selected ? query.error?.message : "Choose a month";
  const previous = query.isError && Boolean(query.data?.data.events.length);

  function changeMonth(value: string) {
    setMonth(value);
    setShown(100);
  }

  return (
    <Panel>
      <div className="flex flex-wrap items-center gap-3 border-b p-5">
        <div className="flex w-full items-center gap-1.5 lg:w-auto">
          <Button
            variant="outline"
            size="icon"
            aria-label="Previous month"
            onClick={() => changeMonth(shiftMonth(month, -1))}
          >
            <ChevronLeft className="size-4" />
          </Button>
          <Input
            aria-label="Month"
            type="month"
            min="0001-01"
            max="9999-12"
            value={month}
            onChange={(event) => changeMonth(event.target.value)}
            className="w-44 flex-1 text-sm font-medium lg:flex-none"
          />
          <Button
            variant="outline"
            size="icon"
            aria-label="Next month"
            onClick={() => changeMonth(shiftMonth(month, 1))}
          >
            <ChevronRight className="size-4" />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="ml-1 text-muted-foreground"
            onClick={() => changeMonth(currentMonth())}
          >
            Today
          </Button>
        </div>
        <Choice
          label="Calendar source"
          value={source || "__all__"}
          onChange={(value) => setSource(value === "__all__" ? "" : value)}
          options={[
            { label: "All sources", value: "__all__" },
            ...status.sources
              .filter((item) => item.calendar)
              .map((item) => ({ label: item.name, value: item.name })),
          ]}
          className="w-36"
        />
        <div className="relative min-w-28 flex-1">
          <Search className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            aria-label="Search events"
            type="search"
            placeholder="Search"
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            className="pl-9"
          />
        </div>
        <Button
          variant="outline"
          aria-label="Refresh calendar from upstream"
          disabled={query.isFetching || !selected}
          onClick={() => query.refresh()}
        >
          <RotateCw
            className={cn("size-4", query.isFetching && "animate-spin")}
          />
          <span className="hidden sm:inline">Refresh</span>
        </Button>
      </div>
      <div className="flex flex-wrap items-center justify-between gap-3 px-5 py-3.5">
        <div className="flex flex-wrap gap-2">
          {data?.sources.map((item) => (
            <Badge
              variant="outline"
              key={item.source}
              className={cn(
                "gap-2 font-normal",
                (item.status !== "ok" || item.cache === "stale") &&
                  "border-primary/25 text-primary",
              )}
            >
              <span
                className={cn(
                  "size-1.5 rounded-full bg-emerald-400",
                  item.status !== "ok" && "bg-destructive",
                )}
              />
              {item.source}
              <span className="text-muted-foreground">
                {sourceState(item, elapsed)}
              </span>
            </Badge>
          ))}
        </div>
        <span className="flex items-center gap-1.5 text-xs text-muted-foreground">
          <Clock3 className="size-3" />
          UTC
        </span>
      </div>
      {error && (
        <div className="px-5 pb-4">
          <Notice>
            {error}
            {previous ? " · previous results" : ""}
          </Notice>
        </div>
      )}
      {query.isFetching && !data?.events.length && (
        <EmptyState icon={CalendarDays} loading title="Loading calendar…" />
      )}
      {!query.isFetching && events.length === 0 && !error && (
        <EmptyState
          icon={CalendarDays}
          title={
            search || source ? "No matching releases" : "No releases this month"
          }
        />
      )}
      {events.length > 0 && <CalendarEvents events={events.slice(0, shown)} />}
      {events.length > shown && (
        <div className="border-t p-4 text-center">
          <Button
            variant="outline"
            size="sm"
            onClick={() => setShown(shown + 100)}
          >
            Show more ({events.length - shown})
          </Button>
        </div>
      )}
    </Panel>
  );
}

function failedCalendar(error: unknown): CalendarData | undefined {
  if (!(error instanceof HttpError) || error.status !== 502) {
    return;
  }

  const data = error.data;

  if (
    data &&
    typeof data === "object" &&
    "events" in data &&
    "sources" in data &&
    Array.isArray(data.events) &&
    data.events.length === 0 &&
    Array.isArray(data.sources) &&
    data.sources.every(
      (source: unknown) =>
        source !== null &&
        typeof source === "object" &&
        "source" in source &&
        typeof source.source === "string" &&
        "status" in source &&
        source.status === "unavailable",
    )
  ) {
    return data as CalendarData;
  }
}

function shiftMonth(month: string, amount: number) {
  const valid = isMonth(month) ? month : currentMonth();
  const [year, number] = valid.split("-").map(Number);
  const date = new Date(0);

  date.setUTCFullYear(year ?? 2000, (number ?? 1) - 1 + amount, 1);

  if (date.getUTCFullYear() < 1 || date.getUTCFullYear() > 9999) {
    return valid;
  }

  return `${String(date.getUTCFullYear()).padStart(4, "0")}-${String(date.getUTCMonth() + 1).padStart(2, "0")}`;
}

function sourceState(source: CalendarData["sources"][number], elapsed: number) {
  if (source.status !== "ok") {
    return "Unavailable";
  }

  const age =
    source.age_seconds == null
      ? "age unknown"
      : `${source.age_seconds + elapsed}s`;

  return `${source.cache ?? "unknown"} · ${age}`;
}
