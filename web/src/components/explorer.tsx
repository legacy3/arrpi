"use client";

import { useSetState } from "ahooks";
import { Code, RefreshCw } from "lucide-react";

import type { Api, Status } from "@/lib/api";
import type { Catalog } from "@/lib/explorer";

import { Choice } from "@/components/choice";
import { EmptyState } from "@/components/empty-state";
import { CommandForm } from "@/components/explorer-commands";
import { ReadOperation } from "@/components/explorer-read";
import { Notice } from "@/components/notice";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { HttpError } from "@/lib/api";
import { operations } from "@/lib/explorer";
import { useApiQuery } from "@/lib/use-api-query";

export function Explorer({
  api,
  status,
}: Readonly<{ api: Api; status: Status }>) {
  const [selected, select] = useSetState({ operation: "", source: "*" });
  const catalog = useApiQuery<Catalog>(api, "/v1/tools");
  const inventory = useApiQuery<Status>(api, "/v1/status", {
    initialData: status,
  });
  const commandsDisabled =
    catalog.error instanceof HttpError && catalog.error.status === 404;
  const source = status.sources.find((item) => item.name === selected.source);
  const available = operations(
    status.sources,
    source,
    catalog.data?.data.tools ?? [],
  );
  const operation =
    available.find((item) => item.id === selected.operation) ?? available[0];

  let content = (
    <EmptyState
      icon={Code}
      loading={catalog.isFetching}
      title={
        catalog.isFetching ? "Loading operations…" : "No permitted operations"
      }
    />
  );

  if (operation?.type === "command") {
    content = (
      <CommandForm
        key={`${source?.name ?? "*"}:${operation.id}`}
        api={api}
        command={operation.command}
        source={source?.name}
      />
    );
  } else if (operation) {
    content = (
      <ReadOperation
        key={`${source?.name ?? "*"}:${operation.id}`}
        api={api}
        source={source?.name}
        operation={operation}
      />
    );
  }

  return (
    <>
      <div className="flex flex-wrap items-end gap-4">
        <div className="grid min-w-0 flex-1 gap-2 sm:flex-none">
          <Label htmlFor="explorer-source">Source</Label>
          <Choice
            id="explorer-source"
            label="Source"
            value={source?.name ?? "*"}
            onChange={(value) => select({ operation: "", source: value })}
            options={[
              { label: "All sources", value: "*" },
              ...status.sources.map((item) => ({
                label: item.name,
                value: item.name,
              })),
            ]}
            className="w-full sm:w-48"
          />
        </div>
        {operation && (
          <div className="grid min-w-0 flex-1 gap-2 sm:flex-none">
            <Label htmlFor="explorer-operation">Operation</Label>
            <Choice
              id="explorer-operation"
              label="Operation"
              value={operation.id}
              onChange={(value) => select({ operation: value })}
              options={available.map((item) => ({
                label: item.label,
                value: item.id,
              }))}
              className="w-full sm:w-72"
            />
          </div>
        )}
        <Button
          variant="ghost"
          size="icon-sm"
          disabled={catalog.isFetching || inventory.isFetching}
          aria-label="Refresh available operations"
          onClick={() => {
            catalog.refresh();
            inventory.refresh();
          }}
        >
          <RefreshCw className="size-4" />
        </Button>
      </div>
      {catalog.data?.data.account &&
        (source?.account ||
          (operation?.type === "command" && operation.command.source)) && (
          <p className="text-sm text-muted-foreground">
            Account: {catalog.data.data.account}
          </p>
        )}
      {content}
      {catalog.error && !commandsDisabled && (
        <Notice>{catalog.error.message}</Notice>
      )}
      {source?.account && commandsDisabled && (
        <Notice tone="info">
          Enable MCP in Settings to use account commands.
        </Notice>
      )}
      {!operation &&
        !catalog.isFetching &&
        !catalog.error &&
        source?.account &&
        !catalog.data?.data.account && (
          <Notice tone="info">
            A requester account must be linked before commands are available.
          </Notice>
        )}
    </>
  );
}
