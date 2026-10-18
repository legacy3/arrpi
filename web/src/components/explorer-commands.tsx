"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useResetState } from "ahooks";
import { Play } from "lucide-react";
import { useState } from "react";

import type { Api } from "@/lib/api";
import type { Command } from "@/lib/explorer";

import {
  CommandArguments,
  guidedCommands,
} from "@/components/command-arguments";
import {
  CommandProgress,
  commandReceipt,
  type CommandTarget,
} from "@/components/command-progress";
import { JsonResponse } from "@/components/json-response";
import { Notice } from "@/components/notice";
import { Panel } from "@/components/panel";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Label } from "@/components/ui/label";
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Textarea } from "@/components/ui/textarea";
import { HttpError } from "@/lib/api";
import { parseArguments } from "@/lib/explorer";
import { useSession } from "@/lib/session-context";

export function CommandForm({
  api,
  command,
  source,
}: Readonly<{ api: Api; command: Command; source?: string }>) {
  const client = useQueryClient();
  const { status } = useSession();
  const selectedSource =
    source && command.sources?.includes(source) ? source : undefined;
  const maintenance = (command.sources?.length ?? 0) > 0;
  const [input, setInput] = useState(
    selectedSource ? JSON.stringify({ source: selectedSource }, null, 2) : "{}",
  );
  const [mode, setMode] = useState(
    guidedCommands.has(command.name) ? "guided" : "json",
  );
  const [progress, setProgress, closeProgress] =
    useResetState<CommandTarget | null>(null);
  let parsed: Record<string, unknown> = {};

  try {
    parsed = parseArguments(input);
  } catch {
    /* Advanced JSON validation runs on submit. */
  }

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
      const args = parseArguments(input);

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

  const receipt = commandReceipt(mutation.data?.data);
  const argumentSource =
    typeof parsed.source === "string" &&
    command.sources?.includes(parsed.source)
      ? parsed.source
      : selectedSource;
  const series =
    status.sources.find((entry) => entry.name === argumentSource)?.kind ===
    "sonarr";

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
            {readOnly
              ? "Read only"
              : `Writes to ${argumentSource ?? command.source ?? "this account"}`}
          </Badge>
        </div>
        <p className="text-sm text-muted-foreground">
          {mode === "guided"
            ? command.description.split(". ")[0]
            : command.description}
        </p>
        {mode === "guided" && (
          <details className="text-xs text-muted-foreground">
            <summary className="cursor-pointer">Command details</summary>
            <p className="mt-2 leading-relaxed">{command.description}</p>
          </details>
        )}
        {guidedCommands.has(command.name) && (
          <Tabs value={mode} onValueChange={setMode}>
            <TabsList aria-label="Argument editor">
              <TabsTrigger value="guided">Form</TabsTrigger>
              <TabsTrigger value="json">Advanced JSON</TabsTrigger>
            </TabsList>
          </Tabs>
        )}
        <fieldset disabled={mutation.isPending} className="grid gap-4">
          {mode === "guided" ? (
            <CommandArguments
              api={api}
              command={command}
              lockedSource={selectedSource}
              series={series}
              value={parsed}
              onChange={(value) => edit(JSON.stringify(value, null, 2))}
            />
          ) : (
            <>
              <details>
                <summary className="cursor-pointer text-sm">
                  Input schema
                </summary>
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
            </>
          )}
        </fieldset>
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
                {maintenance
                  ? "Apply this change to the library"
                  : "Apply this change to my account"}
              </Label>
            </div>
          )}
          <Button
            type="submit"
            className="sm:ml-auto"
            disabled={mutation.isPending || (!readOnly && !confirmed)}
          >
            <Play className="size-4" />
            {mutation.isPending ? "Running…" : "Run command"}
          </Button>
        </div>
        {receipt && (
          <div className="pb-4">
            <Button
              type="button"
              variant="outline"
              onClick={() => setProgress(receipt)}
            >
              Check command progress
            </Button>
          </div>
        )}
        <CommandProgress api={api} target={progress} onClose={closeProgress} />
        {mutation.error && <Notice>{mutation.error.message}</Notice>}
      </form>
      {mutation.data && <JsonResponse value={mutation.data.data} />}
      {mutation.error instanceof HttpError && mutation.error.data !== null && (
        <JsonResponse label="JSON error response" value={mutation.error.data} />
      )}
    </Panel>
  );
}
