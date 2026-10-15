"use client";

import { RefreshCw } from "lucide-react";
import Link from "next/link";
import { useState } from "react";

import type { Api } from "@/lib/api";
import type { Components as components } from "@/lib/generated/contracts";
import type { SettingsEnvelope } from "@/lib/settings";

import { McpAudit } from "@/components/mcp-audit";
import { McpConnections } from "@/components/mcp-connections";
import { Notice } from "@/components/notice";
import { Panel } from "@/components/panel";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { useApiQuery } from "@/lib/use-api-query";

export type McpStatus = components["schemas"]["McpStatus"];

export function Mcp({ api }: Readonly<{ api: Api }>) {
  const query = useApiQuery<SettingsEnvelope>(api, "/v1/config");
  const [account, setAccount] = useState("");

  if (query.error) {
    return <Notice>{query.error.message}</Notice>;
  }

  if (!query.data) {
    return <p className="text-muted-foreground">Loading MCP…</p>;
  }

  return (
    <McpActivity
      key={query.data.data.revision}
      api={api}
      document={query.data.data}
      selected={
        query.data.data.settings.clients.some(
          (client) => client.name === account,
        )
          ? account
          : (query.data.data.settings.clients.find(
              (client) => client.seerr_user,
            )?.name ??
            query.data.data.settings.clients[0]?.name ??
            "")
      }
      onSelect={(name) => {
        setAccount(name);
      }}
    />
  );
}

function connectionStatus(status: McpStatus | undefined) {
  if (!status) {
    return "Checking Seerr…";
  }

  if (!status.connected) {
    return status.connection_error === "upstream_unavailable"
      ? "Seerr unavailable"
      : "No verified Seerr connection";
  }

  return status.writes_supported
    ? "Seerr connected"
    : "Seerr connected · read-only version";
}

function McpActivity({
  api,
  document,
  onSelect,
  selected,
}: Readonly<{
  api: Api;
  document: SettingsEnvelope;
  selected: string;
  onSelect: (name: string) => void;
}>) {
  const { settings } = document;
  const status = useApiQuery<McpStatus>(
    api,
    `/v1/mcp/status?client=${encodeURIComponent(selected)}`,
    { enabled: Boolean(selected), keepPrevious: true },
  );
  const statistics = useApiQuery<components["schemas"]["ToolStatsPage"]>(
    api,
    `/v1/config/mcp/stats?client=${encodeURIComponent(selected)}`,
    { enabled: Boolean(selected), keepPrevious: true },
  );
  const endpoint =
    settings.mcp.public_url ??
    `${typeof window === "undefined" ? "http://localhost:3000" : window.location.origin}/v1/mcp`;

  return (
    <>
      <McpConnections
        api={api}
        ready={Boolean(settings.seerr) && settings.mcp.enabled}
        clients={settings.clients}
        endpoint={endpoint}
        selected={selected}
        onSelect={onSelect}
      />
      {!settings.seerr && (
        <Notice tone="info">
          Configure the{" "}
          <Link href="/settings" className="underline">
            Seerr connection in Settings
          </Link>{" "}
          to enable MCP.
        </Notice>
      )}
      {status.data?.data.connected && !status.data.data.writes_supported && (
        <Notice tone="info">
          Seerr's version is unverified. Read tools work; requests and issues
          are disabled.
        </Notice>
      )}
      {!settings.mcp.enabled && settings.seerr && (
        <Notice tone="info">MCP is disabled. Enable it in MCP settings.</Notice>
      )}
      {status.error && <Notice>{status.error.message}</Notice>}
      {statistics.error && <Notice>{statistics.error.message}</Notice>}
      <McpAudit
        key={selected}
        api={api}
        client={selected}
        pageLimit={settings.mcp.limits.max_page_items}
      />
      <Panel
        title="Tool usage"
        detail={
          <Button
            type="button"
            variant="ghost"
            size="sm"
            aria-label="Refresh tool usage"
            onClick={() => {
              status.refresh();
              statistics.refresh();
            }}
            disabled={status.isFetching || statistics.isFetching}
          >
            <RefreshCw className="size-4" />
            Refresh
          </Button>
        }
      >
        <div className="flex flex-wrap items-center gap-3 px-5 pb-4">
          <span className="text-xs text-muted-foreground">
            {connectionStatus(
              status.isPlaceholderData ? undefined : status.data?.data,
            )}
          </span>
        </div>
        <Table aria-label="Tools">
          <TableHeader>
            <TableRow>
              <TableHead className="pl-5">Tool</TableHead>
              <TableHead>Account access</TableHead>
              <TableHead className="text-right">Calls</TableHead>
              <TableHead className="text-right">Errors</TableHead>
              <TableHead>Last used</TableHead>
              <TableHead className="pr-5 text-right">Enabled</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {status.data?.data.tools.map((tool) => (
              <TableRow key={tool.name}>
                <TableCell className="max-w-lg whitespace-normal py-3 pl-5">
                  <span className="break-all font-mono text-xs">
                    {tool.name}
                  </span>
                  <p
                    className="mt-1 max-w-lg whitespace-normal text-xs text-muted-foreground"
                    title={tool.description}
                  >
                    {tool.description.split(". ")[0]}
                  </p>
                </TableCell>
                <TableCell className="text-xs text-muted-foreground">
                  {toolAccess(tool.available, status.isPlaceholderData)}
                </TableCell>
                <ToolStatistics
                  tool={tool.name}
                  items={
                    statistics.isPlaceholderData
                      ? undefined
                      : statistics.data?.data.items
                  }
                />
                <TableCell className="pr-5 text-right">
                  {settings.mcp.disabled_tools.includes(tool.name)
                    ? "Disabled"
                    : "Enabled"}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </Panel>
    </>
  );
}

function toolAccess(permitted: boolean, loading: boolean) {
  if (loading) {
    return "Checking…";
  }

  return permitted ? "Permitted" : "Not permitted";
}

function ToolStatistics({
  items,
  tool,
}: Readonly<{
  tool: string;
  items: components["schemas"]["ToolStat"][] | undefined;
}>) {
  const rows = items?.filter((row) => row.tool === tool);
  const calls = rows?.reduce((total, row) => total + row.calls, 0);
  const errors = rows
    ?.filter((row) => row.outcome !== "ok")
    .reduce((total, row) => total + row.calls, 0);
  const last = rows
    ?.map((row) => row.last_used)
    .sort()
    .at(-1);

  return (
    <>
      <TableCell className="text-right tabular-nums">{calls ?? "—"}</TableCell>
      <TableCell className="text-right tabular-nums">{errors ?? "—"}</TableCell>
      <TableCell className="text-xs text-muted-foreground">
        {last ? new Date(last).toLocaleString() : "—"}
      </TableCell>
    </>
  );
}
