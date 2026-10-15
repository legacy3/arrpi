// Generated from Rust configuration schemas; run pnpm types. Do not edit.
export const numericFields = {
  cache: {
    connection_idle_seconds: { default: 30, max: 300, min: 5 },
    connection_lifetime_seconds: { default: 300, max: 3600, min: 30 },
    fresh_seconds: { default: 60, max: 86400, min: 1 },
    max_body_bytes: { default: 1048576, max: 67108864, min: 1024 },
    max_connections: { default: 64, max: 256, min: 1 },
    max_entries: { default: 32, max: 1024, min: 1 },
    max_normalizations: { default: 4, max: 16, min: 1 },
    max_requests: { default: 64, max: 256, min: 1 },
    max_retained_bytes: { default: 134217728, max: 2147483648, min: 1048576 },
    max_upstream_requests: { default: 4, max: 64, min: 1 },
    retry_seconds: { default: 5, max: 60, min: 1 },
    stale_seconds: { default: 900, max: 86400, min: 0 },
    timeout_seconds: { default: 10, max: 60, min: 1 },
  },
  mcp: {
    audit_max_entries: { default: 10000, max: 100000, min: 1 },
    batch_timeout_seconds: { default: 45, max: 300, min: 1 },
    max_batch_items: { default: 20, max: 100, min: 1 },
    max_concurrent_writes: { default: 4, max: 32, min: 1 },
    max_page_items: { default: 100, max: 500, min: 1 },
    queue_timeout_seconds: { default: 15, max: 300, min: 1 },
  },
  subscriptions: {
    future_days: { default: 60, max: 92, min: 0 },
    max_subjects: { default: 4096, max: 4096, min: 1 },
    past_days: { default: 7, max: 92, min: 0 },
  },
} as const;
