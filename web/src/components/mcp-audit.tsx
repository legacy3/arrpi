"use client";

import { RefreshCw } from "lucide-react";
import { useState } from "react";

import type { Api } from "@/lib/api";
import type { Components as components } from "@/lib/generated/contracts";

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

type AuditPage = components["schemas"]["AuditPage"];
type AuditTarget = components["schemas"]["AuditTarget"];

export function McpAudit({
  api,
  client,
  pageLimit,
}: Readonly<{ api: Api; client: string; pageLimit: number }>) {
  const [offset, setOffset] = useState(0);
  const limit = Math.min(20, pageLimit);
  const query = useApiQuery<AuditPage>(
    api,
    `/v1/config/mcp/audit?client=${encodeURIComponent(client)}&offset=${offset}&limit=${limit}`,
    { enabled: Boolean(client) },
  );
  const data = query.data?.data;
  const entryLabel = data?.total === 1 ? "entry" : "entries";

  return (
    <Panel
      title="Write history"
      detail={
        <Button
          type="button"
          variant="ghost"
          size="sm"
          aria-label="Refresh write history"
          disabled={query.isFetching}
          onClick={() => query.refresh()}
        >
          <RefreshCw className="size-4" />
          Refresh
        </Button>
      }
    >
      {query.error ? (
        <Notice>{query.error.message}</Notice>
      ) : (
        <>
          <Table aria-label="Write history">
            <TableHeader>
              <TableRow>
                <TableHead className="pl-5">When</TableHead>
                <TableHead>Action</TableHead>
                <TableHead>Media / issue</TableHead>
                <TableHead className="pr-5">Outcome</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {data?.items.map((row) => (
                <TableRow key={row.id}>
                  <TableCell className="pl-5 text-xs">
                    {new Date(row.created_at).toLocaleString()}
                  </TableCell>
                  <TableCell className="font-mono text-xs">
                    {row.tool}
                  </TableCell>
                  <TableCell className="text-xs">
                    {targetLabel(row.target)}
                  </TableCell>
                  <TableCell className="pr-5 font-mono text-xs">
                    {row.outcome}
                  </TableCell>
                </TableRow>
              ))}
              {data?.items.length === 0 && (
                <TableRow>
                  <TableCell
                    colSpan={4}
                    className="px-5 py-4 text-sm text-muted-foreground"
                  >
                    No writes yet.
                  </TableCell>
                </TableRow>
              )}
            </TableBody>
          </Table>
          <div className="flex items-center justify-between px-5 py-4">
            <span className="text-xs text-muted-foreground">
              {data ? `${data.total} ${entryLabel}` : "Loading…"}
            </span>
            <div className="flex gap-2">
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={offset === 0 || query.isFetching}
                onClick={() => setOffset(Math.max(0, offset - limit))}
              >
                Previous
              </Button>
              <Button
                type="button"
                variant="outline"
                size="sm"
                disabled={data?.next_offset == null || query.isFetching}
                onClick={() => {
                  if (data?.next_offset != null) {
                    setOffset(data.next_offset);
                  }
                }}
              >
                Next
              </Button>
            </div>
          </div>
        </>
      )}
    </Panel>
  );
}

function targetLabel(target: AuditTarget) {
  return (
    [
      target.tmdb_id == null ? null : `TMDB ${target.tmdb_id}`,
      target.item_id == null
        ? null
        : `${target.source ?? "library"} #${target.item_id}`,
      target.command_id == null ? null : `command #${target.command_id}`,
      target.seasons?.length ? `S${target.seasons.join(", ")}` : null,
      target.request_id == null ? null : `request #${target.request_id}`,
      target.issue_id == null ? null : `issue #${target.issue_id}`,
    ]
      .filter(Boolean)
      .join(" · ") || "—"
  );
}
