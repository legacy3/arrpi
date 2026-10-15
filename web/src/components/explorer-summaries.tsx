"use client";

import { Code, Play } from "lucide-react";
import { useState } from "react";

import type { Api, SummaryKind } from "@/lib/api";

import { Choice } from "@/components/choice";
import { EmptyState } from "@/components/empty-state";
import { JsonResponse } from "@/components/json-response";
import { Notice } from "@/components/notice";
import { Panel } from "@/components/panel";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { type Summary, summaryLabels } from "@/lib/summaries";
import { useApiQuery } from "@/lib/use-api-query";

export function SummaryExplorer({
  api,
  kinds,
}: Readonly<{ api: Api; kinds: SummaryKind[] }>) {
  const [selected, setSelected] = useState(kinds[0]);
  const kind = kinds.find((item) => item === selected) ?? kinds[0];
  const [fresh, setFresh] = useState(false);
  const [requested, setRequested] = useState("");
  const path = `/v1/summaries/${kind}`;
  const query = useApiQuery<Summary>(api, requested || path, {
    enabled: Boolean(requested),
  });
  const match = requested.startsWith(`${path}?`);

  function fetchResponse() {
    const next = `${path}?no_cache=${fresh}`;

    if (next === requested) {
      query.refresh();
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
          <Label htmlFor="explorer-summary">Projection</Label>
          <Choice
            id="explorer-summary"
            label="Projection"
            value={kind ?? ""}
            options={kinds.map((item) => ({
              label: summaryLabels[item],
              value: item,
            }))}
            onChange={(value) => {
              setSelected(kinds.find((item) => item === value));
              setRequested("");
            }}
          />
        </div>
        <div className="flex h-9 items-center gap-2.5 sm:ml-auto">
          <Checkbox
            id="summary-fresh"
            checked={fresh}
            onCheckedChange={(value) => setFresh(value === true)}
          />
          <Label htmlFor="summary-fresh">Fresh upstream read</Label>
        </div>
        <Button disabled={query.isFetching}>
          <Play className="size-4" />
          Fetch
        </Button>
      </form>
      <p className="border-b px-5 py-3 font-mono text-xs text-muted-foreground">
        GET {path}
      </p>
      {match && query.error && (
        <div className="p-5">
          <Notice>{query.error.message}</Notice>
        </div>
      )}
      {match && query.data ? (
        <JsonResponse value={query.data.data} />
      ) : (
        <EmptyState
          icon={Code}
          loading={query.isFetching}
          title={query.isFetching ? "Fetching…" : "No response yet"}
        />
      )}
    </Panel>
  );
}
