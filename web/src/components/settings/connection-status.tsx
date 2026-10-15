import { useQuery } from "@tanstack/react-query";

import type { Api } from "@/lib/api";
import type { Components } from "@/lib/generated/contracts";

import { cn } from "@/lib/utils";

type ConnectionHealth = Components["schemas"]["ConnectionHealth"];

const states = {
  attention: { label: "Needs attention", tone: "bg-amber-500" },
  authentication_failed: {
    label: "Authentication failed",
    tone: "bg-destructive",
  },
  check_failed: { label: "Check failed", tone: "bg-amber-500" },
  checking: { label: "Checking…", tone: "animate-pulse bg-muted-foreground" },
  healthy: { label: "Healthy", tone: "bg-emerald-500" },
  unavailable: { label: "Unavailable", tone: "bg-destructive" },
  unconfigured: { label: "Not configured", tone: "bg-muted-foreground" },
} as const;

export function ConnectionStatus({
  api,
  configured,
  revision,
  source,
}: Readonly<{
  api: Api;
  configured: boolean;
  revision: number;
  source: string;
}>) {
  const query = useQuery({
    enabled: configured,
    queryFn: async ({ signal }) =>
      (
        await api.read<ConnectionHealth>(
          `/v1/config/connections/${encodeURIComponent(source)}/health`,
          true,
          signal,
        )
      ).data,
    queryKey: ["connection-health", revision, source],
    retry: false,
    staleTime: 0,
  });
  let state: keyof typeof states = "unconfigured";

  if (configured) {
    state = query.data?.status ?? "checking";

    if (query.error) {
      state = "check_failed";
    }

    if (query.isFetching) {
      state = "checking";
    }
  }

  const { label, tone } = states[state];
  const isConfigured = configured && state !== "unconfigured";

  return (
    <span
      role="img"
      className="ml-auto flex shrink-0 items-center gap-2 text-xs text-muted-foreground"
      aria-label={`${source}: ${isConfigured ? "Configured · " : ""}${label}`}
    >
      <span
        aria-hidden="true"
        className={cn("size-1.5 shrink-0 rounded-full", tone)}
      />
      <span>
        {isConfigured && (
          <span className="hidden sm:inline">Configured · </span>
        )}
        {label}
      </span>
    </span>
  );
}
