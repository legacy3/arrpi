import { numericFields } from "@/lib/generated/form-constraints";
import { bytes } from "@/lib/summaries";

export type PolicyKey = keyof typeof numericFields.cache;
export const policyLabels: Record<PolicyKey, string> = {
  connection_idle_seconds: "Idle timeout (seconds)",
  connection_lifetime_seconds: "Connection lifetime (seconds)",
  fresh_seconds: "Fresh data (seconds)",
  max_body_bytes: "Maximum response (bytes)",
  max_connections: "Maximum connections",
  max_entries: "Cached queries per source",
  max_normalizations: "Concurrent projections",
  max_requests: "Active API responses",
  max_retained_bytes: "Shared cache budget (bytes)",
  max_upstream_requests: "Upstream requests per source",
  retry_seconds: "Retry delay (seconds)",
  stale_seconds: "Stale fallback (seconds)",
  timeout_seconds: "Upstream timeout (seconds)",
};
export const policyGroups = [
  {
    keys: [
      "fresh_seconds",
      "stale_seconds",
      "retry_seconds",
      "max_entries",
      "max_body_bytes",
      "max_retained_bytes",
    ],
    title: "Cache",
  },
  {
    keys: [
      "timeout_seconds",
      "max_upstream_requests",
      "max_requests",
      "max_normalizations",
    ],
    title: "Requests",
  },
  {
    keys: [
      "max_connections",
      "connection_idle_seconds",
      "connection_lifetime_seconds",
    ],
    title: "HTTP listener",
  },
] as const;
export const policyHelp: Record<PolicyKey, string> = {
  connection_idle_seconds: "Close an idle connection after this interval.",
  connection_lifetime_seconds: "Maximum lifetime of one connection.",
  fresh_seconds: "How long a cached read remains fresh.",
  max_body_bytes: "Largest accepted service response.",
  max_connections: "Maximum open client connections. Requires a restart.",
  max_entries: "Maximum cached queries in each source.",
  max_normalizations: "Maximum simultaneous library and calendar projections.",
  max_requests: "Maximum simultaneous API responses.",
  max_retained_bytes: "Shared memory budget for cached and retained responses.",
  max_upstream_requests: "Maximum concurrent reads to each service.",
  retry_seconds: "Delay before retrying a failed service read.",
  stale_seconds: "How long an older result may be used when its service fails.",
  timeout_seconds: "Maximum time to wait for a service response.",
};
export function policyValue(key: PolicyKey, value: number) {
  if (key.endsWith("_bytes")) {
    return bytes(value);
  }

  if (key.endsWith("_seconds")) {
    return `${value}s`;
  }

  return value.toLocaleString();
}
