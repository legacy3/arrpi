"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Code, Play, RefreshCw } from "lucide-react";
import { useState } from "react";

import type { Api } from "@/lib/api";
import type { Components } from "@/lib/generated/contracts";

import { Choice } from "@/components/choice";
import { EmptyState } from "@/components/empty-state";
import { JsonResponse } from "@/components/json-response";
import { Notice } from "@/components/notice";
import { Panel } from "@/components/panel";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { HttpError } from "@/lib/api";
import { useApiQuery } from "@/lib/use-api-query";

type Command = Components["schemas"]["CommandDescriptor"];

export function CommandExplorer({ api }: Readonly<{ api: Api }>) {
  const query = useApiQuery<Components["schemas"]["CommandCatalog"]>(
    api,
    "/v1/tools",
  );
  const [selected, setSelected] = useState("");
  const commands = query.data?.data.tools ?? [];
  const command =
    commands.find((item) => item.name === selected) ?? commands[0];
  let content = (
    <EmptyState
      icon={Code}
      loading={query.isFetching}
      title={query.isFetching ? "Loading commands…" : "No permitted commands"}
    />
  );

  if (command && !query.error) {
    content = (
      <>
        <div className="grid gap-2 border-t p-5">
          <Label htmlFor="explorer-command">Command</Label>
          <Choice
            id="explorer-command"
            label="Command"
            value={command.name}
            onChange={setSelected}
            options={commands.map((item) => ({
              label: item.name,
              value: item.name,
            }))}
            className="w-full sm:w-72"
          />
        </div>
        <CommandForm key={command.name} api={api} command={command} />
      </>
    );
  }

  return (
    <Panel
      title="Shared commands"
      detail={
        <Button
          variant="ghost"
          size="icon-sm"
          disabled={query.isFetching}
          aria-label="Refresh available commands"
          onClick={() => query.refresh()}
        >
          <RefreshCw className="size-3.5" />
        </Button>
      }
    >
      {query.error ? (
        <div className="px-5 pb-5">
          <Notice
            tone={
              query.error instanceof HttpError && query.error.status === 404
                ? "info"
                : "error"
            }
          >
            {query.error instanceof HttpError && query.error.status === 404
              ? "Shared commands require an enabled MCP service and a configured Seerr connection. Operational summaries remain available independently."
              : query.error.message}
          </Notice>
        </div>
      ) : (
        content
      )}
    </Panel>
  );
}

function CommandForm({
  api,
  command,
}: Readonly<{ api: Api; command: Command }>) {
  const client = useQueryClient();
  const [input, setInput] = useState("{}");
  const [fresh, setFresh] = useState(false);
  const [confirmed, setConfirmed] = useState(false);
  const readOnly = command.annotations?.readOnlyHint === true;
  const properties = command.inputSchema.properties;
  const acceptsFresh =
    properties !== null &&
    typeof properties === "object" &&
    Object.hasOwn(properties, "no_cache");
  const mutation = useMutation({
    async mutationFn() {
      const value: unknown = JSON.parse(input);

      if (!value || typeof value !== "object" || Array.isArray(value)) {
        throw new Error("Arguments must be a JSON object");
      }

      const args = value as Record<string, unknown>;

      return api.command<unknown>(
        command.name,
        acceptsFresh
          ? { ...args, no_cache: fresh || args.no_cache === true }
          : args,
      );
    },
    onMutate() {
      setConfirmed(false);
    },
    onSuccess() {
      if (!readOnly) {
        void client.invalidateQueries({
          predicate: (query) =>
            typeof query.queryKey[0] === "string" &&
            ["/v1/summaries/", "/v1/calendar", "/v1/config/mcp/"].some(
              (prefix) => query.queryKey[0]?.toString().startsWith(prefix),
            ),
        });
      }
    },
    retry: false,
  });

  function edit(value: string) {
    setInput(value);
    setConfirmed(false);
    mutation.reset();
  }

  return (
    <>
      <form
        className="grid gap-4 border-t p-5"
        onSubmit={(event) => {
          event.preventDefault();

          if (!mutation.isPending && (readOnly || confirmed)) {
            mutation.mutate();
          }
        }}
      >
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant="outline">POST</Badge>
          <code className="break-all text-xs text-muted-foreground">
            /v1/tools/{command.name}
          </code>
          <Badge variant={readOnly ? "secondary" : "outline"}>
            {readOnly ? "Read only" : "Writes to Seerr"}
          </Badge>
        </div>
        <p className="text-sm text-muted-foreground">{command.description}</p>
        <details>
          <summary className="cursor-pointer text-sm">Input schema</summary>
          <JsonResponse
            label="Command input schema"
            value={command.inputSchema}
          />
        </details>
        <div className="grid gap-2">
          <Label htmlFor="command-arguments">Arguments (JSON)</Label>
          <Textarea
            id="command-arguments"
            value={input}
            onChange={(event) => edit(event.target.value)}
            disabled={mutation.isPending}
            spellCheck={false}
            className="min-h-32 font-mono text-xs"
          />
        </div>
        <div className="flex flex-wrap items-center gap-4">
          {acceptsFresh && (
            <div className="flex items-center gap-2">
              <Checkbox
                id="command-fresh"
                checked={fresh}
                disabled={mutation.isPending}
                onCheckedChange={(value) => {
                  setFresh(value === true);
                  mutation.reset();
                }}
              />
              <Label htmlFor="command-fresh">Fresh upstream read</Label>
            </div>
          )}
          {!readOnly && (
            <div className="flex items-center gap-2">
              <Checkbox
                id="command-confirm"
                checked={confirmed}
                disabled={mutation.isPending}
                onCheckedChange={(value) => setConfirmed(value === true)}
              />
              <Label htmlFor="command-confirm">
                Apply this change to my Seerr account
              </Label>
            </div>
          )}
          <Button
            className="sm:ml-auto"
            disabled={mutation.isPending || (!readOnly && !confirmed)}
          >
            <Play className="size-4" />
            {mutation.isPending ? "Running…" : "Run command"}
          </Button>
        </div>
        {mutation.error && <Notice>{mutation.error.message}</Notice>}
      </form>
      {mutation.data && <JsonResponse value={mutation.data.data} />}
      {mutation.error instanceof HttpError && mutation.error.data !== null && (
        <JsonResponse label="JSON error response" value={mutation.error.data} />
      )}
    </>
  );
}
