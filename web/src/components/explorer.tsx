"use client";

import { Code, Play } from "lucide-react";
import { useState } from "react";

import { Choice } from "@/components/choice";
import { EmptyState } from "@/components/empty-state";
import { CommandExplorer } from "@/components/explorer-commands";
import { SummaryExplorer } from "@/components/explorer-summaries";
import { JsonResponse } from "@/components/json-response";
import { Notice } from "@/components/notice";
import { Panel } from "@/components/panel";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  type Api,
  currentMonth,
  dateRange,
  type Endpoint,
  type Source,
  type Status,
} from "@/lib/api";
import { summaryKinds } from "@/lib/summaries";
import { useApiQuery } from "@/lib/use-api-query";

export function Explorer({
  api,
  status,
}: Readonly<{ api: Api; status: Status }>) {
  const sources = status.sources.filter((item) => item.endpoints.length > 0);
  const summaries = summaryKinds(status.sources);
  const interfaces = [
    ...(sources.length > 0
      ? [{ label: "Native endpoints", value: "native" }]
      : []),
    ...(summaries.length > 0
      ? [{ label: "Operational summaries", value: "summaries" }]
      : []),
    { label: "Shared commands", value: "commands" },
  ];
  const [selected, setSelected] = useState(interfaces[0]?.value ?? "commands");
  const view = interfaces.some((item) => item.value === selected)
    ? selected
    : interfaces[0]?.value;
  const [sourceName, setSourceName] = useState(sources[0]?.name ?? "");
  const source = sources.find((item) => item.name === sourceName) ?? sources[0];

  return (
    <>
      <div className="flex flex-wrap items-center gap-3">
        <Label htmlFor="explorer-interface">Interface</Label>
        <Choice
          id="explorer-interface"
          label="Interface"
          value={view ?? "commands"}
          onChange={setSelected}
          options={interfaces}
          className="w-56"
        />
      </div>
      {view === "commands" && <CommandExplorer api={api} />}
      {view === "summaries" && <SummaryExplorer api={api} kinds={summaries} />}
      {view === "native" && source && (
        <NativeExplorer
          api={api}
          source={source.name}
          sources={sources}
          endpoints={source.endpoints}
          onSourceChange={setSourceName}
        />
      )}
    </>
  );
}

function NativeExplorer({
  api,
  endpoints,
  onSourceChange,
  source,
  sources,
}: Readonly<{
  api: Api;
  source: string;
  sources: Source[];
  onSourceChange: (value: string) => void;
  endpoints: Endpoint[];
}>) {
  const [endpoint, setEndpoint] = useState<Endpoint>(endpoints[0] ?? "health");
  const [month, setMonth] = useState(currentMonth);
  const [page, setPage] = useState("1");
  const [fresh, setFresh] = useState(false);
  const [requested, setRequested] = useState("");
  const [validationError, setValidationError] = useState("");
  const path = `/v1/${encodeURIComponent(source)}/api/v3/${encodeURIComponent(endpoint)}`;
  const query = useApiQuery<unknown>(api, requested || path, {
    enabled: Boolean(requested),
    initialFresh: fresh,
  });
  const match = requested.startsWith(`${path}?`);
  const response = match ? query.data : undefined;
  const fetching = match && query.isFetching;
  const error = validationError || (match ? query.error?.message : "");

  function fetchResponse() {
    setValidationError("");
    let parameters: Record<string, string> = {};

    if (endpoint === "calendar") {
      try {
        parameters = dateRange(month);
      } catch (error_) {
        setValidationError(
          error_ instanceof Error ? error_.message : "Choose a month",
        );

        return;
      }
    }

    if (endpoint === "queue") {
      parameters = { page, pageSize: "20" };
    }

    const next = `${path}?${new URLSearchParams(parameters)}`;

    if (next === requested) {
      query.refresh(fresh);
    } else {
      setRequested(next);
    }
  }

  return (
    <Panel>
      <form
        className="flex flex-wrap items-end gap-4 border-b p-5"
        onSubmit={(event) => {
          event.preventDefault();
          fetchResponse();
        }}
      >
        <div className="grid gap-2">
          <Label htmlFor="explorer-source">Source</Label>
          <Choice
            id="explorer-source"
            label="Source"
            value={source}
            onChange={(next) => {
              const target = sources.find((item) => item.name === next);

              if (target) {
                setEndpoint(target.endpoints[0] ?? "health");
                setRequested("");
                setValidationError("");
                onSourceChange(next);
              }
            }}
            options={sources.map((item) => ({
              label: item.name,
              value: item.name,
            }))}
          />
        </div>
        <div className="grid gap-2">
          <Label htmlFor="explorer-endpoint">Endpoint</Label>
          <Choice
            id="explorer-endpoint"
            label="Endpoint"
            value={endpoint}
            onChange={(next) => {
              const value = endpoints.find((item) => item === next);

              if (value) {
                setEndpoint(value);
                setRequested("");
                setValidationError("");
              }
            }}
            options={endpoints.map((item) => ({ label: item, value: item }))}
          />
        </div>
        {endpoint === "calendar" && (
          <div className="grid gap-2">
            <Label htmlFor="explorer-month">Month</Label>
            <Input
              id="explorer-month"
              type="month"
              min="0001-01"
              max="9999-12"
              required
              value={month}
              onChange={(event) => setMonth(event.target.value)}
              className="w-40"
            />
          </div>
        )}
        {endpoint === "queue" && (
          <div className="grid gap-2">
            <Label htmlFor="explorer-page">Page</Label>
            <Input
              id="explorer-page"
              type="number"
              required
              min={1}
              max={100}
              value={page}
              onChange={(event) => setPage(event.target.value)}
              className="w-20"
            />
          </div>
        )}
        <div className="flex h-9 items-center gap-2.5 sm:ml-auto">
          <Checkbox
            id="fresh"
            checked={fresh}
            onCheckedChange={(checked) => setFresh(checked === true)}
          />
          <Label
            htmlFor="fresh"
            className="text-xs font-normal text-muted-foreground"
          >
            Fresh upstream read
          </Label>
        </div>
        <Button disabled={fetching}>
          <Play className="size-4" />
          {fetching ? "Fetching…" : "Fetch"}
        </Button>
      </form>
      <div className="flex flex-wrap items-center justify-between gap-3 border-b bg-muted/20 px-5 py-3">
        <div className="flex min-w-0 items-center gap-2.5">
          <Badge
            variant="outline"
            className="border-emerald-400/20 font-mono text-emerald-300"
          >
            GET
          </Badge>
          <code className="truncate font-mono text-xs text-muted-foreground">
            {path}
          </code>
        </div>
        {response && (
          <Badge variant="secondary" className="font-mono text-xs">
            {response.cache ?? "unknown"} ·{" "}
            {response.age === null ? "age unknown" : `${response.age}s`}
          </Badge>
        )}
      </div>
      {error && (
        <div className="p-5">
          <Notice>{error}</Notice>
        </div>
      )}
      {!error && response && (
        <JsonResponse label="Native JSON response" value={response.data} />
      )}
      {!error && !response && (
        <EmptyState
          icon={Code}
          loading={fetching}
          title={fetching ? "Fetching…" : "No response yet"}
        />
      )}
    </Panel>
  );
}
