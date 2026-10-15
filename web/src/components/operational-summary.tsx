"use client";

import { RefreshCw } from "lucide-react";

import type { Api, SummaryKind } from "@/lib/api";

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
import {
  type Summary,
  summaryLabels,
  summaryMetrics,
  summaryWarnings,
} from "@/lib/summaries";
import { useApiQuery } from "@/lib/use-api-query";
import { useElapsed } from "@/lib/use-elapsed";

export function OperationalSummary({
  api,
  kind,
}: Readonly<{ api: Api; kind: SummaryKind }>) {
  const query = useApiQuery<Summary>(api, `/v1/summaries/${kind}`);
  const result = query.data?.data;
  const elapsed = useElapsed(query.data?.receivedAt);

  return (
    <Panel
      title={summaryLabels[kind]}
      detail={
        <Button
          variant="ghost"
          size="icon-sm"
          disabled={query.isFetching}
          aria-label={`Refresh ${summaryLabels[kind].toLowerCase()} from upstream`}
          onClick={() => query.refresh(true)}
        >
          <RefreshCw className="size-3.5" />
        </Button>
      }
    >
      {query.error && (
        <div className="px-5 pb-4">
          <Notice>
            {query.error.message}
            {result ? " · previous results" : ""}
          </Notice>
        </div>
      )}
      {!result && !query.error && (
        <p className="px-5 pb-5 text-sm text-muted-foreground">Loading…</p>
      )}
      {result && (
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="pl-5">Source</TableHead>
              <TableHead>Activity</TableHead>
              <TableHead className="pr-5 text-right">Observation</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {result.sources.map((row) => (
              <TableRow key={row.source}>
                <TableCell className="pl-5 align-top font-medium">
                  {row.source}
                </TableCell>
                <TableCell className="min-w-52 whitespace-normal">
                  {row.status === "unavailable" ? (
                    <span className="text-destructive">
                      Unavailable · {row.code}
                    </span>
                  ) : (
                    <>
                      <dl className="flex flex-wrap gap-x-6 gap-y-2">
                        {summaryMetrics(row).map((metric) => (
                          <div key={metric.label}>
                            <dt className="text-xs text-muted-foreground">
                              {metric.label}
                            </dt>
                            <dd className="text-sm tabular-nums">
                              {metric.value}
                            </dd>
                          </div>
                        ))}
                      </dl>
                      {summaryWarnings(row).map((warning) => (
                        <p
                          key={warning}
                          className="mt-2 text-xs text-amber-300"
                        >
                          {warning}
                        </p>
                      ))}
                    </>
                  )}
                </TableCell>
                <TableCell className="pr-5 align-top text-right text-xs text-muted-foreground">
                  {row.status === "ok" ? (
                    <span>
                      {row.freshness.cache}
                      <br />
                      {typeof row.freshness.age_seconds === "number"
                        ? `${row.freshness.age_seconds + elapsed}s old`
                        : "Age unknown"}
                    </span>
                  ) : (
                    "—"
                  )}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      )}
    </Panel>
  );
}
