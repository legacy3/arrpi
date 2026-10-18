"use client";

import {
  CalendarDays,
  ChevronLeft,
  ChevronRight,
  Clock3,
  RefreshCw,
} from "lucide-react";
import { parseAsInteger, parseAsString, useQueryStates } from "nuqs";
import { useMemo } from "react";

import { CalendarEvents } from "@/components/calendar-events";
import { Choice } from "@/components/choice";
import { EmptyState } from "@/components/empty-state";
import { Notice } from "@/components/notice";
import { Panel } from "@/components/panel";
import { SearchField } from "@/components/search-field";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useFuzzySearch } from "@/hooks/use-fuzzy-search";
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

const CALENDAR_KEYS = ["title", "series", "source"] as const;

export function Calendar({
  api,
  status,
}: Readonly<{ api: Api; status: Status }>) {
  const [filters, setFilters] = useQueryStates({
    month: parseAsString.withDefault(currentMonth()),
    search: parseAsString.withDefault(""),
    shown: parseAsInteger.withDefault(100),
    source: parseAsString.withDefault(""),
  });
  const { month, search, source } = filters;
  const shown = Math.max(100, filters.shown);
  const setMonth = (month: string) =>
    void setFilters({ month }, { history: "push" });
  const setSource = (source: string) => void setFilters({ shown: 100, source });
  const setSearch = (search: string) => void setFilters({ search, shown: 100 });
  const setShown = (shown: number) => void setFilters({ shown });
  const selected = isMonth(month);
  const dates = dateRange(selected ? month : "2000-01");
  const query = useApiQuery<CalendarData>(
    api,
    `/v1/calendar?${new URLSearchParams(dates)}`,
    { enabled: selected },
  );
  const data = query.data?.data ?? failedCalendar(query.error);
  const elapsed = useElapsed(query.data?.receivedAt);
  const all = useMemo(() => data?.events ?? [], [data?.events]);
  const matches = useFuzzySearch({
    items: all,
    keys: CALENDAR_KEYS,
    limit: 10_000,
    query: search,
  });
  const events = matches.filter((event) => !source || event.source === source);
  const error = selected ? query.error?.message : "Choose a month";
  const previous = query.isError && Boolean(query.data?.data.events.length);

  function changeMonth(value: string) {
    setMonth(value);
    setShown(100);
  }

  return (
    <Panel>
      <div className="grid grid-cols-[minmax(0,1fr)_2.25rem_2.25rem] items-center gap-2 border-b p-5 min-[360px]:grid-cols-[7rem_minmax(0,1fr)_2.25rem] lg:flex lg:flex-wrap lg:gap-3">
        <div className="col-span-3 flex min-w-0 items-center gap-1.5 min-[360px]:col-span-2 lg:w-auto">
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="outline"
                size="icon"
                aria-label="Previous month"
                onClick={() => changeMonth(shiftMonth(month, -1))}
              >
                <ChevronLeft className="size-4" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Previous month</TooltipContent>
          </Tooltip>
          <Input
            aria-label="Month"
            type="month"
            min="0001-01"
            max="9999-12"
            value={month}
            onChange={(event) => changeMonth(event.target.value)}
            className="w-44 flex-1 px-2 text-sm font-medium lg:flex-none"
          />
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="outline"
                size="icon"
                aria-label="Next month"
                onClick={() => changeMonth(shiftMonth(month, 1))}
              >
                <ChevronRight className="size-4" />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Next month</TooltipContent>
          </Tooltip>
        </div>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              aria-label="This month"
              className="col-start-2 row-start-2 w-9 px-0 text-muted-foreground min-[360px]:col-start-3 min-[360px]:row-start-1 xl:w-auto xl:px-3.5"
              onClick={() => changeMonth(currentMonth())}
            >
              <CalendarDays className="size-4" />
              <span className="hidden xl:inline">This month</span>
            </Button>
          </TooltipTrigger>
          <TooltipContent>This month</TooltipContent>
        </Tooltip>
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
          className="col-start-1 row-start-2 w-full gap-1 px-2 lg:w-32"
        />
        <div className="relative col-span-3 row-start-3 min-w-0 min-[360px]:col-span-1 min-[360px]:col-start-2 min-[360px]:row-start-2 lg:min-w-32 lg:flex-1">
          <SearchField
            label="Search events"
            placeholder="Search"
            value={search}
            onChange={setSearch}
          />
        </div>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="outline"
              aria-label="Refresh calendar from upstream"
              className="col-start-3 row-start-2 w-9 px-0 xl:w-auto xl:px-3.5"
              disabled={query.isFetching || !selected}
              onClick={() => query.refresh()}
            >
              <RefreshCw
                className={cn("size-4", query.isFetching && "animate-spin")}
              />
              <span className="hidden xl:inline">Refresh</span>
            </Button>
          </TooltipTrigger>
          <TooltipContent>Refresh calendar</TooltipContent>
        </Tooltip>
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
            {previous ? " (previous result)" : ""}
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

  return `${source.cache ?? "unknown"}, ${age}`;
}
