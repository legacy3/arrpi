"use client";

import { CircleAlert, Clock3, RefreshCw } from "lucide-react";

import type { Api, SummaryKind } from "@/lib/api";

import { Notice } from "@/components/notice";
import { Panel } from "@/components/panel";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import { Button } from "@/components/ui/button";
import { appInfo, errorText } from "@/lib/presentation";
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
            {result && <p className="mt-1">Showing the previous result.</p>}
          </Notice>
        </div>
      )}
      {!result && !query.error && (
        <p className="px-5 pb-5 text-sm text-muted-foreground">Loading...</p>
      )}
      {result && (
        <ul className="divide-y border-t">
          {result.sources.map((row) => {
            const AppIcon = appInfo(row.source).icon;

            return (
              <li key={row.source} className="space-y-3 px-5 py-3">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span className="flex items-center gap-2 font-medium">
                    <AppIcon className="size-4 text-muted-foreground" />
                    {row.source}
                  </span>
                  {row.status === "ok" && (
                    <span className="flex items-center gap-1 text-xs text-muted-foreground">
                      <Clock3 className="size-3" />
                      {typeof row.freshness.age_seconds === "number"
                        ? `${row.freshness.age_seconds + elapsed}s old`
                        : "Age unknown"}
                      {row.freshness.cache === "stale" && (
                        <span className="ml-1 text-amber-300">Stale</span>
                      )}
                    </span>
                  )}
                </div>
                {row.status === "unavailable" ? (
                  <Notice>
                    <span className="font-medium">
                      {row.source} is unavailable.
                    </span>{" "}
                    {errorText(row.code)}
                    <Accordion type="multiple" className="mt-1">
                      <AccordionItem value="error">
                        <AccordionTrigger className="py-1 text-xs">
                          Technical details
                        </AccordionTrigger>
                        <AccordionContent>
                          <code>{row.code}</code>
                        </AccordionContent>
                      </AccordionItem>
                    </Accordion>
                  </Notice>
                ) : (
                  <>
                    <dl className="grid grid-cols-2 gap-x-5 gap-y-2 sm:grid-cols-3">
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
                        className="flex items-start gap-1.5 text-xs leading-relaxed text-amber-200"
                      >
                        <CircleAlert className="mt-0.5 size-3.5 shrink-0" />
                        {warning}
                      </p>
                    ))}
                  </>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </Panel>
  );
}
