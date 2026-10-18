import type { Endpoint, Source, SummaryKind } from "@/lib/api";
import type { Components } from "@/lib/generated/contracts";

import { endpointLabels } from "@/lib/presentation";
import { summaryKinds, summaryLabels } from "@/lib/summaries";

export type Catalog = Components["schemas"]["CommandCatalog"];
export type Command = Components["schemas"]["CommandDescriptor"];

export type Operation =
  | { id: string; label: string; type: "native"; endpoint: Endpoint }
  | { id: string; label: string; type: "summary"; summary: SummaryKind }
  | { id: string; label: string; type: "command"; command: Command };

export function operations(
  sources: Source[],
  source: Source | undefined,
  commands: Command[],
): Operation[] {
  return [
    ...(source?.endpoints ?? []).map((endpoint) => ({
      endpoint,
      id: `native:${endpoint}`,
      label: endpointLabels[endpoint] ?? endpoint,
      type: "native" as const,
    })),
    ...summaryKinds(source ? [source] : sources).map((summary) => ({
      id: `summary:${summary}`,
      label: `${summaryLabels[summary]} summary`,
      summary,
      type: "summary" as const,
    })),
    ...commands
      .filter(
        (command) =>
          !source ||
          command.source === source.name ||
          command.sources?.includes(source.name),
      )
      .toSorted(
        (left, right) =>
          Number(right.annotations?.readOnlyHint === true) -
            Number(left.annotations?.readOnlyHint === true) ||
          left.name.localeCompare(right.name),
      )
      .map((command) => ({
        command,
        id: `command:${command.name}`,
        label: command.title,
        type: "command" as const,
      })),
  ];
}

export function parseArguments(input: string): Record<string, unknown> {
  const value: unknown = JSON.parse(input);

  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("Arguments must be a JSON object");
  }

  return value as Record<string, unknown>;
}

export function readPath(source: string | undefined, operation: Operation) {
  if (operation.type === "native" && source) {
    return `/v1/${encodeURIComponent(source)}/api/v3/${operation.endpoint}`;
  }

  if (operation.type === "summary") {
    return source
      ? `/v1/sources/${encodeURIComponent(source)}/summaries/${operation.summary}`
      : `/v1/summaries/${operation.summary}`;
  }

  throw new Error("Not a readable operation");
}
