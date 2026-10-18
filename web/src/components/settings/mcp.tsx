"use client";

import Link from "next/link";
import { parseAsString, useQueryState } from "nuqs";
import { z } from "zod";

import type { McpStatus } from "@/components/mcp";

import { Notice } from "@/components/notice";
import { SearchField } from "@/components/search-field";
import { Checkbox } from "@/components/ui/checkbox";
import { useFuzzySearch } from "@/hooks/use-fuzzy-search";
import { numericFields } from "@/lib/generated/form-constraints";
import { useApiQuery } from "@/lib/use-api-query";

import { type EditorProps, useSettingsForm } from "./form";
import { boundedNumber, httpUrl } from "./schemas";

const TOOL_KEYS = ["title", "name", "description"] as const;
const limitLabels: Record<keyof typeof numericFields.mcp, string> = {
  audit_max_entries: "Write history entries per account",
  batch_timeout_seconds: "Write batch budget (seconds)",
  max_batch_items: "Items per batch",
  max_concurrent_writes: "Concurrent writes per source",
  max_page_items: "Items per page",
  max_search_episodes: "Episodes per automatic search",
  queue_timeout_seconds: "Queue wait (seconds)",
};
const limitNames = Object.keys(
  numericFields.mcp,
) as (keyof typeof numericFields.mcp)[];
const schema = z.object({
  disabled_tools: z.array(z.string()),
  enabled: z.boolean(),
  limits: z.object(
    Object.fromEntries(
      limitNames.map((name) => [name, boundedNumber(numericFields.mcp[name])]),
    ) as Record<keyof typeof numericFields.mcp, z.ZodNumber>,
  ),
  public_url: z.union([
    z.literal(""),
    httpUrl.refine((value) => {
      if (!URL.canParse(value)) {
        return false;
      }

      const url = new URL(value);

      return url.protocol === "https:" && url.pathname === "/v1/mcp";
    }, "Use an HTTPS /v1/mcp endpoint"),
  ]),
});

export function McpSettings(props: EditorProps) {
  const { settings } = props.document;
  const client =
    settings.clients.find((entry) => entry.seerr_user) ?? settings.clients[0];
  const registry = useApiQuery<McpStatus>(
    props.api,
    `/v1/mcp/status?client=${encodeURIComponent(client?.name ?? "")}`,
    { enabled: Boolean(client) },
  );

  return (
    <div className="max-w-3xl space-y-4">
      <h2 className="text-lg font-semibold">MCP server</h2>
      {!settings.seerr && (
        <Notice tone="info">
          Configure the{" "}
          <Link href="/settings" className="underline">
            Seerr connection in Settings
          </Link>{" "}
          to enable MCP.
        </Notice>
      )}
      {registry.error && <Notice>{registry.error.message}</Notice>}
      <McpEditor
        key={props.document.revision}
        {...props}
        tools={registry.data?.data.tools ?? []}
        loading={registry.isFetching && !registry.data}
      />
    </div>
  );
}

function McpEditor(
  props: EditorProps &
    Readonly<{ tools: McpStatus["tools"]; loading: boolean }>,
) {
  const { settings } = props.document;
  const [search, setSearch] = useQueryState(
    "tools",
    parseAsString.withDefault(""),
  );
  const matches = useFuzzySearch({
    items: props.tools,
    keys: TOOL_KEYS,
    query: search,
  });
  const { form, mutation } = useSettingsForm(
    props,
    {
      disabled_tools: settings.mcp.disabled_tools,
      enabled: settings.mcp.enabled,
      limits: settings.mcp.limits,
      public_url: settings.mcp.public_url ?? "",
    },
    schema,
    (value) => ({
      ...settings,
      mcp: {
        ...settings.mcp,
        disabled_tools: value.disabled_tools,
        enabled: value.enabled,
        limits: { ...settings.mcp.limits, ...value.limits },
        public_url: value.public_url || null,
      },
      oauth:
        settings.oauth && !value.enabled
          ? { ...settings.oauth, enabled: false }
          : settings.oauth,
    }),
  );

  return (
    <form.AppForm>
      <form.Editor mutation={mutation} message={props.message}>
        <form.AppField name="enabled">
          {(field) => (
            <field.Toggle
              label="Enable MCP"
              disabled={!settings.seerr && !field.state.value}
            />
          )}
        </form.AppField>
        {settings.oauth?.enabled && (
          <p className="text-xs text-muted-foreground">
            Disabling MCP also disables OAuth. Identity bindings are kept.
          </p>
        )}
        <form.AppField name="public_url">
          {(field) => (
            <field.Text
              label="Public endpoint"
              type="url"
              placeholder="https://arrpi.example.com/v1/mcp"
            />
          )}
        </form.AppField>
        <details className="rounded-lg border p-4">
          <summary className="cursor-pointer font-medium">
            Advanced limits
          </summary>
          <div className="mt-4 grid gap-4 sm:grid-cols-2">
            {limitNames.map((name) => (
              <form.AppField key={name} name={`limits.${name}`}>
                {(field) => (
                  <field.Number
                    label={limitLabels[name]}
                    {...numericFields.mcp[name]}
                  />
                )}
              </form.AppField>
            ))}
          </div>
        </details>
        <details className="rounded-lg border p-4">
          <summary className="cursor-pointer font-medium">
            Tools ({props.tools.length})
          </summary>
          <div className="mt-4 grid gap-3">
            <SearchField
              label="Search tool switches"
              value={search}
              onChange={(value) => void setSearch(value)}
            />
            <p className="text-xs text-muted-foreground">
              These switches apply to all accounts. Account grants and Seerr
              permissions still apply.
            </p>
            {props.loading && (
              <p role="status" className="text-muted-foreground">
                Loading tools…
              </p>
            )}
            <form.AppField name="disabled_tools">
              {(field) => (
                <div className="grid gap-x-5 sm:grid-cols-2">
                  {matches.map((tool) => (
                    <label
                      key={tool.name}
                      className="flex items-start gap-3 py-3"
                    >
                      <Checkbox
                        className="mt-1"
                        aria-label={`Enable ${tool.name}`}
                        checked={!field.state.value.includes(tool.name)}
                        onBlur={field.handleBlur}
                        onCheckedChange={(checked) =>
                          field.handleChange(
                            checked === true
                              ? field.state.value.filter(
                                  (name) => name !== tool.name,
                                )
                              : [...field.state.value, tool.name],
                          )
                        }
                      />
                      <span className="min-w-0">
                        <span className="text-sm">{tool.title}</span>
                        <span className="mt-1 block text-xs text-muted-foreground">
                          {tool.name}
                        </span>
                      </span>
                    </label>
                  ))}
                </div>
              )}
            </form.AppField>
          </div>
        </details>
      </form.Editor>
    </form.AppForm>
  );
}
