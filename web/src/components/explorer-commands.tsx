"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Play } from "lucide-react";
import { useState } from "react";

import type { Api } from "@/lib/api";
import type { Command } from "@/lib/explorer";

import { JsonResponse } from "@/components/json-response";
import { Notice } from "@/components/notice";
import { Panel } from "@/components/panel";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import { HttpError } from "@/lib/api";

export function CommandForm({
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
            [
              "/v1/summaries/",
              "/v1/sources/",
              "/v1/calendar",
              "/v1/config/mcp/",
            ].some((prefix) =>
              query.queryKey[0]?.toString().startsWith(prefix),
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
    <Panel>
      <form
        className="grid gap-4 p-5"
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
            {readOnly ? "Read only" : `Writes to ${command.source ?? "source"}`}
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
                Apply this change to my account
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
    </Panel>
  );
}
