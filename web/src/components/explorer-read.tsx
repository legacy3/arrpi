"use client";

import { useSetState } from "ahooks";
import { Code, Play } from "lucide-react";

import type { Api } from "@/lib/api";
import type { Operation } from "@/lib/explorer";

import { EmptyState } from "@/components/empty-state";
import { JsonResponse } from "@/components/json-response";
import { Notice } from "@/components/notice";
import { Panel } from "@/components/panel";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { currentMonth, dateRange } from "@/lib/api";
import { readPath } from "@/lib/explorer";
import { useApiQuery } from "@/lib/use-api-query";

export function ReadOperation({
  api,
  operation,
  source,
}: Readonly<{ api: Api; source?: string; operation: Operation }>) {
  const [form, update] = useSetState({
    error: "",
    fresh: false,
    month: currentMonth(),
    page: "1",
    requested: "",
  });
  const endpoint = operation.type === "native" ? operation.endpoint : undefined;
  const path = readPath(source, operation);
  const query = useApiQuery<unknown>(api, form.requested || path, {
    enabled: Boolean(form.requested),
    initialFresh: form.fresh,
  });
  const response = form.requested ? query.data : undefined;
  const fetching = Boolean(form.requested) && query.isFetching;
  const error =
    form.error || (form.requested ? query.error?.message : undefined);

  function fetchResponse() {
    try {
      const parameters = new URLSearchParams(
        endpoint === "calendar" ? dateRange(form.month) : {},
      );

      if (endpoint === "queue") {
        parameters.set("page", form.page);
        parameters.set("pageSize", "20");
      }

      const requested = `${path}?${parameters}`;

      update({ error: "" });

      if (requested === form.requested) {
        query.refresh(form.fresh);
      } else {
        update({ requested });
      }
    } catch (error) {
      update({
        error: error instanceof Error ? error.message : "Invalid input",
      });
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
        {endpoint === "calendar" && (
          <div className="grid gap-2">
            <Label htmlFor="explorer-month">Month</Label>
            <Input
              id="explorer-month"
              type="month"
              min="0001-01"
              max="9999-12"
              required
              value={form.month}
              onChange={(event) => update({ month: event.target.value })}
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
              min={1}
              max={100}
              required
              value={form.page}
              onChange={(event) => update({ page: event.target.value })}
              className="w-20"
            />
          </div>
        )}
        <div className="flex h-9 items-center gap-2.5 sm:ml-auto">
          <Checkbox
            id="explorer-fresh"
            checked={form.fresh}
            onCheckedChange={(value) => update({ fresh: value === true })}
          />
          <Label htmlFor="explorer-fresh">Fresh upstream read</Label>
        </div>
        <Button disabled={fetching}>
          <Play className="size-4" />
          {fetching ? "Fetching…" : "Fetch"}
        </Button>
      </form>
      <div className="flex flex-wrap items-center justify-between gap-3 border-b px-5 py-3">
        <code className="break-all text-xs text-muted-foreground">
          GET {path}
        </code>
        {response?.cache && !error && (
          <Badge variant="secondary">
            {response.cache}
            {response.age === null ? "" : ` · ${response.age}s`}
          </Badge>
        )}
      </div>
      {error && (
        <div className="p-5">
          <Notice>{error}</Notice>
        </div>
      )}
      {response && !error ? (
        <JsonResponse
          label={
            operation.type === "native"
              ? "Native JSON response"
              : "JSON response"
          }
          value={response.data}
        />
      ) : (
        !error && (
          <EmptyState
            icon={Code}
            loading={fetching}
            title={fetching ? "Fetching…" : "No response yet"}
          />
        )
      )}
    </Panel>
  );
}
