"use client";

import { CircleAlert, CircleCheck, RefreshCw } from "lucide-react";
import Link from "next/link";
import {
  parseAsInteger,
  parseAsString,
  parseAsStringLiteral,
  useQueryStates,
} from "nuqs";
import { useMemo } from "react";

import type { Api } from "@/lib/api";
import type { Components } from "@/lib/generated/contracts";
import type { SettingsEnvelope } from "@/lib/settings";

import { Choice } from "@/components/choice";
import { McpAudit } from "@/components/mcp-audit";
import { McpConnections } from "@/components/mcp-connections";
import { Notice } from "@/components/notice";
import { Panel } from "@/components/panel";
import { SearchField } from "@/components/search-field";
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
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { useFuzzySearch } from "@/hooks/use-fuzzy-search";
import { errorText, fullTime, shortTime } from "@/lib/presentation";
import { useApiQuery } from "@/lib/use-api-query";

export type McpStatus = Components["schemas"]["McpStatus"];
const SEARCH_KEYS = ["name", "title", "description"] as const;
const accessReasons: Record<string, string> = {
  account_not_linked: "Link a Seerr account",
  account_permission: "Not allowed by Seerr",
  account_unverified: "Verify linked account",
  missing_grant: "Needs an access grant",
  seerr_unavailable: "Seerr unavailable",
  server_disabled: "MCP disabled",
  tool_disabled: "Disabled for all accounts",
  unverified_version: "Seerr version unverified",
};

type UiState = {
  view: "activity" | "tools";
  tools: string;
  access: "all" | "available" | "unavailable";
};

export function Mcp({ api }: Readonly<{ api: Api }>) {
  const query = useApiQuery<SettingsEnvelope>(api, "/v1/config");
  const [state, setState] = useQueryStates({
    access: parseAsStringLiteral([
      "all",
      "available",
      "unavailable",
    ]).withDefault("all"),
    account: parseAsString.withDefault(""),
    offset: parseAsInteger.withDefault(0),
    tools: parseAsString.withDefault(""),
    view: parseAsStringLiteral(["activity", "tools"]).withDefault("activity"),
  });

  if (query.error) {
    return <Notice>{query.error.message}</Notice>;
  }

  if (!query.data) {
    return (
      <p role="status" className="text-muted-foreground">
        Loading MCP...
      </p>
    );
  }

  const { settings } = query.data.data;
  const selected = settings.clients.some(
    (client) => client.name === state.account,
  )
    ? state.account
    : (settings.clients.find((client) => client.seerr_user)?.name ??
      settings.clients[0]?.name ??
      "");

  return (
    <McpActivity
      api={api}
      document={query.data.data}
      selected={selected}
      state={state}
      onSelect={(account) => void setState({ account, offset: 0 })}
      onView={(view) => void setState({ view })}
      onSearch={(tools) => void setState({ tools })}
      onAccess={(access) => void setState({ access })}
    />
  );
}

function McpActivity({
  api,
  document,
  onAccess,
  onSearch,
  onSelect,
  onView,
  selected,
  state,
}: Readonly<{
  api: Api;
  document: SettingsEnvelope;
  selected: string;
  state: UiState;
  onSelect: (name: string) => void;
  onView: (value: "activity" | "tools") => void;
  onSearch: (value: string) => void;
  onAccess: (value: UiState["access"]) => void;
}>) {
  const { settings } = document;
  const status = useApiQuery<McpStatus>(
    api,
    `/v1/mcp/status?client=${encodeURIComponent(selected)}`,
    { enabled: Boolean(selected) },
  );
  const statistics = useApiQuery<Components["schemas"]["ToolStatsPage"]>(
    api,
    `/v1/config/mcp/stats?client=${encodeURIComponent(selected)}`,
    { enabled: Boolean(selected) },
  );
  const tools = useMemo(
    () => status.data?.data.tools ?? [],
    [status.data?.data.tools],
  );
  const matches = useFuzzySearch({
    items: tools,
    keys: SEARCH_KEYS,
    query: state.tools,
  });
  const filtered = matches.filter(
    (tool) =>
      state.access === "all" ||
      tool.available === (state.access === "available"),
  );
  const endpoint =
    settings.mcp.public_url ??
    `${typeof window === "undefined" ? "http://localhost:3000" : window.location.origin}/v1/mcp`;
  const live = status.data?.data;
  const ready = settings.mcp.enabled && Boolean(settings.seerr);

  return (
    <>
      <McpConnections
        api={api}
        ready={ready}
        clients={settings.clients}
        endpoint={endpoint}
        selected={selected}
        onSelect={onSelect}
      />
      <div role="status" className="flex flex-wrap items-center gap-2 text-sm">
        {live?.connected ? (
          <CircleCheck className="size-4 text-emerald-300" />
        ) : (
          <CircleAlert className="size-4 text-amber-300" />
        )}
        <span>{readiness(settings, live, status.isError)}</span>
        {(!ready || (live && !live.connected)) && (
          <Link
            href={
              !settings.seerr || live?.connection_error
                ? "/settings?entry=seerr"
                : `/settings/access?entry=${encodeURIComponent(selected)}`
            }
            className="text-primary underline"
          >
            Review settings
          </Link>
        )}
      </div>
      {status.error && <Notice>{status.error.message}</Notice>}
      <Tabs
        value={state.view}
        onValueChange={(value) =>
          onView(value === "tools" ? "tools" : "activity")
        }
      >
        <TabsList aria-label="MCP views">
          <TabsTrigger value="activity">Activity</TabsTrigger>
          <TabsTrigger value="tools">Tools and usage</TabsTrigger>
        </TabsList>
      </Tabs>
      {state.view === "activity" ? (
        <McpAudit
          api={api}
          client={selected}
          pageLimit={settings.mcp.limits.max_page_items}
          tools={tools}
        />
      ) : (
        <Panel
          title="Tool usage"
          detail={
            <Button
              variant="ghost"
              size="icon-sm"
              aria-label="Refresh tool usage"
              disabled={status.isFetching || statistics.isFetching}
              onClick={() => {
                status.refresh();
                statistics.refresh();
              }}
            >
              <RefreshCw className="size-4" />
            </Button>
          }
        >
          <div className="flex flex-wrap gap-2 px-5 pb-4">
            <SearchField
              className="min-w-44 flex-1"
              label="Search tools"
              value={state.tools}
              onChange={onSearch}
            />
            <Choice
              label="Tool access"
              value={state.access}
              onChange={(value) => onAccess(value as UiState["access"])}
              options={[
                { label: "All tools", value: "all" },
                { label: "Available", value: "available" },
                { label: "Needs attention", value: "unavailable" },
              ]}
            />
          </div>
          {statistics.error && (
            <div className="px-5 pb-4">
              <Notice>{statistics.error.message}</Notice>
            </div>
          )}
          <Table aria-label="Tools">
            <TableHeader>
              <TableRow>
                <TableHead className="pl-5">Tool</TableHead>
                <TableHead>Access</TableHead>
                <TableHead className="hidden text-right md:table-cell">
                  Calls
                </TableHead>
                <TableHead className="hidden text-right md:table-cell">
                  Errors
                </TableHead>
                <TableHead className="hidden pr-5 lg:table-cell">
                  Last used
                </TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {filtered.map((tool) => {
                const rows =
                  statistics.data?.data.items.filter(
                    (row) => row.tool === tool.name,
                  ) ?? [];
                const calls = rows.reduce((sum, row) => sum + row.calls, 0);
                const errors = rows
                  .filter((row) => row.outcome !== "ok")
                  .reduce((sum, row) => sum + row.calls, 0);
                const last = rows
                  .map((row) => row.last_used)
                  .toSorted((a, b) => a.localeCompare(b))
                  .at(-1);
                const reason = tool.reason ?? null;

                return (
                  <TableRow key={tool.name}>
                    <TableCell className="w-full max-w-0 whitespace-normal py-3 pl-5">
                      <div className="font-medium">{tool.title}</div>
                      <p
                        className="mt-1 line-clamp-2 text-xs text-muted-foreground"
                        title={tool.description}
                      >
                        {tool.description.split(". ")[0]}
                      </p>
                      <details className="mt-1 text-xs text-muted-foreground">
                        <summary className="cursor-pointer">Details</summary>
                        <code className="break-all">{tool.name}</code>
                        <p className="mt-1">{tool.description}</p>
                      </details>
                      <div className="mt-1 flex flex-wrap gap-3 text-xs text-muted-foreground md:hidden">
                        <span>{statistics.data ? calls : "Unknown"} calls</span>
                        <span>
                          {statistics.data ? errors : "Unknown"} errors
                        </span>
                      </div>
                    </TableCell>
                    <TableCell className="w-36 whitespace-normal align-top">
                      <Badge
                        variant="outline"
                        className="h-auto min-h-5 whitespace-normal text-xs"
                      >
                        {reason
                          ? (accessReasons[reason] ??
                            reason.replaceAll("_", " "))
                          : "Available"}
                      </Badge>
                      {reason && (
                        <Link
                          className="mt-1 block text-xs text-primary hover:underline"
                          href={
                            reason === "tool_disabled" ||
                            reason === "server_disabled"
                              ? "/settings/mcp"
                              : `/settings/access?entry=${encodeURIComponent(selected)}`
                          }
                        >
                          Review access
                        </Link>
                      )}
                    </TableCell>
                    <TableCell className="hidden text-right tabular-nums md:table-cell">
                      {statistics.data ? calls : "Unknown"}
                    </TableCell>
                    <TableCell className="hidden text-right tabular-nums md:table-cell">
                      {statistics.data ? errors : "Unknown"}
                    </TableCell>
                    <TableCell className="hidden pr-5 text-xs text-muted-foreground lg:table-cell">
                      {last ? (
                        <time dateTime={last} title={fullTime(last)}>
                          {shortTime(last)}
                        </time>
                      ) : (
                        "Never"
                      )}
                    </TableCell>
                  </TableRow>
                );
              })}
              {filtered.length === 0 && (
                <TableRow>
                  <TableCell
                    colSpan={5}
                    className="px-5 py-6 text-sm text-muted-foreground"
                  >
                    No matching tools.
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
        </Panel>
      )}
    </>
  );
}

function readiness(
  settings: SettingsEnvelope["settings"],
  live: McpStatus | undefined,
  failed: boolean,
) {
  if (!settings.mcp.enabled) {
    return "MCP is disabled";
  }

  if (!settings.seerr) {
    return "Configure Seerr to enable MCP";
  }

  if (failed) {
    return "Could not check requester access.";
  }

  if (live?.connection_error) {
    return errorText(live.connection_error);
  }

  if (!live) {
    return "Checking requester access...";
  }

  if (!live.connected) {
    return "Requester access needs attention.";
  }

  return live.writes_supported
    ? "Seerr connected. Requests available."
    : "Seerr connected. Request writes are unavailable for this version.";
}
