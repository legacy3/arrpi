// Generated from Rust public DTOs; run pnpm types. Do not edit.
export type paths = Record<string, never>;
export type webhooks = Record<string, never>;
export interface components {
  schemas: {
    /** @description A recorded attempt is observability, not an exactly-once mutation journal. */
    AuditEntry: {
      /** Format: uint64 */
      actor_id: number | null;
      created_at: string;
      /** Format: uint64 */
      id: number;
      outcome: string;
      target: components["schemas"]["AuditTarget"];
      tool: string;
    };
    /** @description Complete pagination through one account's retained write history. */
    AuditPage: {
      client: string;
      items: components["schemas"]["AuditEntry"][];
      /** Format: uint */
      next_offset: number | null;
      /** Format: uint */
      offset: number;
      /** Format: uint64 */
      total: number;
    };
    /** @description Whitelisted coordinates of a write attempt; free text is never recorded. */
    AuditTarget: {
      /** Format: uint32 */
      episode?: number | null;
      /** Format: uint64 */
      issue_id?: number | null;
      media_type?: components["schemas"]["MediaType"] | null;
      /** Format: uint64 */
      request_id?: number | null;
      /** Format: uint32 */
      season?: number | null;
      seasons?: number[] | null;
      /** Format: uint64 */
      tmdb_id?: number | null;
    };
    /** @description Verified Seerr and Plex identities for one `ArrPi` client. */
    Binding: {
      /** Format: uint64 */
      id: number;
      /** Format: uint64 */
      plex_id: number;
    };
    /** @description Bounds for body retention, normalization, upstream admission and cache freshness. */
    CachePolicy: {
      /**
       * Format: uint64
       * @default 30
       */
      connection_idle_seconds: number;
      /**
       * Format: uint64
       * @default 300
       */
      connection_lifetime_seconds: number;
      /**
       * Format: uint64
       * @default 60
       */
      fresh_seconds: number;
      /**
       * Format: uint
       * @default 1048576
       */
      max_body_bytes: number;
      /**
       * Format: uint
       * @default 64
       */
      max_connections: number;
      /**
       * Format: uint
       * @default 32
       */
      max_entries: number;
      /**
       * Format: uint
       * @description Maximum simultaneous library/calendar normalization operations.
       * @default 4
       */
      max_normalizations: number;
      /**
       * Format: uint
       * @default 64
       */
      max_requests: number;
      /**
       * Format: uint
       * @description Aggregate backing-buffer budget across all sources and held responses.
       * @default 134217728
       */
      max_retained_bytes: number;
      /**
       * Format: uint
       * @default 4
       */
      max_upstream_requests: number;
      /**
       * Format: uint64
       * @default 5
       */
      retry_seconds: number;
      /**
       * Format: uint64
       * @default 900
       */
      stale_seconds: number;
      /**
       * Format: uint64
       * @default 10
       */
      timeout_seconds: number;
    };
    /**
     * @description Cache provenance, distinct from current upstream reachability.
     * @enum {string}
     */
    CacheState: "hit" | "miss" | "stale";
    /** @description Monitored releases combined across authorized calendar capabilities. */
    Calendar: {
      events: components["schemas"]["CalendarEvent"][];
      sources: components["schemas"]["SourceStatus"][];
    };
    /** @description A calendar event contains only user-facing metadata and public identifiers. */
    CalendarEvent: {
      date: string;
      /** Format: uint64 */
      episode?: number | null;
      has_file: boolean;
      /** Format: uint64 */
      id: number;
      imdb_id?: string | null;
      kind: components["schemas"]["EventKind"];
      /** Format: uint64 */
      season?: number | null;
      series?: string | null;
      source: string;
      title: string;
      /** Format: uint64 */
      tmdb_id?: number | null;
      /** Format: uint64 */
      tvdb_id?: number | null;
    };
    /** @description `ArrPi` client grants and an optional native requester identity binding. */
    Client: {
      key: components["schemas"]["Credential"];
      name: string;
      permissions: string[];
      /** @default null */
      seerr_user: components["schemas"]["Binding"] | null;
    };
    /** @description Receipt for an applied revision; listener changes require a process restart. */
    ConfigUpdateResult: {
      restart_required: boolean;
      /** Format: uint64 */
      revision: number;
    };
    /** @description Operator-only connection diagnostics without native identities or error details. */
    ConnectionHealth: {
      /** Format: uint */
      health_issue_count: number | null;
      source: string;
      status: components["schemas"]["ConnectionState"];
    };
    /**
     * @description Current probe outcome; configuration alone never establishes reachability.
     * @enum {string}
     */
    ConnectionState:
      | "healthy"
      | "attention"
      | "authentication_failed"
      | "unavailable"
      | "unconfigured";
    /** @description Aggregate source counters without native identities or query labels. */
    Counters: {
      /** Format: uint64 */
      hits: number;
      /** Format: uint64 */
      misses: number;
      /** Format: uint64 */
      stale: number;
      /** Format: uint64 */
      upstream_failures: number;
    };
    Credential: string | null;
    /**
     * @description Public native endpoints supported by the allowlisted gateway.
     * @enum {string}
     */
    Endpoint:
      "calendar" | "series" | "movie" | "queue" | "health" | "qualityprofile";
    /**
     * @description Calendar event type; cinema and home releases are separate events.
     * @enum {string}
     */
    EventKind: "episode" | "cinema" | "digital" | "physical";
    /** @description Public metadata shared by calendar feed renderers. */
    Feed: {
      /** @default https://github.com/legacy3/arrpi */
      link: string;
      /** @default ArrPi release calendar */
      title: string;
    };
    /** @description Configured pagination, batch, write admission and audit retention limits. */
    Limits: {
      /**
       * Format: uint
       * @default 10000
       */
      audit_max_entries: number;
      /**
       * Format: uint64
       * @default 45
       */
      batch_timeout_seconds: number;
      /**
       * Format: uint
       * @default 20
       */
      max_batch_items: number;
      /**
       * Format: uint
       * @default 4
       */
      max_concurrent_writes: number;
      /**
       * Format: uint
       * @default 100
       */
      max_page_items: number;
      /**
       * Format: uint64
       * @default 15
       */
      queue_timeout_seconds: number;
    };
    /** @description Global command availability and execution bounds. */
    McpPolicy: {
      /** @default [] */
      disabled_tools: string[];
      /** @default false */
      enabled: boolean;
      /**
       * @default {
       *       "audit_max_entries": 10000,
       *       "batch_timeout_seconds": 45,
       *       "max_batch_items": 20,
       *       "max_concurrent_writes": 4,
       *       "max_page_items": 100,
       *       "queue_timeout_seconds": 15
       *     }
       */
      limits: components["schemas"]["Limits"];
      /** @default null */
      public_url: string | null;
    };
    /** @description MCP prerequisites and current bound-account tool permissions. */
    McpStatus: {
      configured: boolean;
      connected: boolean;
      connection_error: string | null;
      tools: components["schemas"]["ToolVisibility"][];
      writes_supported: boolean;
    };
    /** @enum {string} */
    MediaType: "movie" | "tv";
    /** @description An administrator-approved identity, never inferred from email or token roles. */
    OAuthIdentityBinding: {
      client: string;
      issuer: string;
      subject: string;
    };
    /** @description Optional HTTP MCP resource-server policy. The external server owns login and tokens. */
    OAuthPolicy: {
      /** @default [] */
      bindings: components["schemas"]["OAuthIdentityBinding"][];
      /** @default false */
      enabled: boolean;
      issuer: string;
      /** @default arrpi:mcp */
      required_scope: string;
    };
    /** @description Catalog and requester source settings; credentials never serialize. */
    SeerrConnection: {
      api_key: components["schemas"]["Credential"];
      /** @default en */
      language: string;
      url: string;
    };
    /** @description Protected application settings stored in the single private `SQLite` document. */
    SettingsDocument: {
      /**
       * @default {
       *       "connection_idle_seconds": 30,
       *       "connection_lifetime_seconds": 300,
       *       "fresh_seconds": 60,
       *       "max_body_bytes": 1048576,
       *       "max_connections": 64,
       *       "max_entries": 32,
       *       "max_normalizations": 4,
       *       "max_requests": 64,
       *       "max_retained_bytes": 134217728,
       *       "max_upstream_requests": 4,
       *       "retry_seconds": 5,
       *       "stale_seconds": 900,
       *       "timeout_seconds": 10
       *     }
       */
      cache: components["schemas"]["CachePolicy"];
      clients: components["schemas"]["Client"][];
      /**
       * @default {
       *       "link": "https://github.com/legacy3/arrpi",
       *       "title": "ArrPi release calendar"
       *     }
       */
      feed: components["schemas"]["Feed"];
      listen: string;
      /**
       * @default {
       *       "disabled_tools": [],
       *       "enabled": false,
       *       "limits": {
       *         "audit_max_entries": 10000,
       *         "batch_timeout_seconds": 45,
       *         "max_batch_items": 20,
       *         "max_concurrent_writes": 4,
       *         "max_page_items": 100,
       *         "queue_timeout_seconds": 15
       *       },
       *       "public_url": null
       *     }
       */
      mcp: components["schemas"]["McpPolicy"];
      /** @default null */
      oauth: components["schemas"]["OAuthPolicy"] | null;
      /** @default null */
      seerr: components["schemas"]["SeerrConnection"] | null;
      sources: {
        [key: string]: components["schemas"]["Source"];
      };
      /**
       * @default {
       *       "enabled": false,
       *       "future_days": 60,
       *       "master_key": null,
       *       "max_subjects": 4096,
       *       "past_days": 7
       *     }
       */
      subscriptions: components["schemas"]["SubscriptionPolicy"];
    };
    /** @description Revisioned settings; credential serialization always emits null. */
    SettingsEnvelope: {
      /** Format: uint64 */
      revision: number;
      settings: components["schemas"]["SettingsDocument"];
    };
    /** @description A read-only native source with an optional Seerr service mapping. */
    Source: {
      api_key: components["schemas"]["Credential"];
      kind: components["schemas"]["SourceKind"];
      /**
       * Format: uint32
       * @default null
       */
      seerr_server_id: number | null;
      url: string;
    };
    /**
     * @description Availability of one source in a combined result.
     * @enum {string}
     */
    SourceAvailability: "ok" | "unavailable";
    /** @description A visible source and its capabilities, without its connection credentials. */
    SourceInfo: {
      calendar: boolean;
      counters: components["schemas"]["Counters"];
      endpoints: components["schemas"]["Endpoint"][];
      kind: string;
      name: string;
    };
    /**
     * @description Supported native library manager protocols.
     * @enum {string}
     */
    SourceKind:
      "sonarr" | "radarr" | "sabnzbd" | "cleanuparr" | "bazarr" | "maintainerr";
    /** @description Per-source calendar result and observation freshness. */
    SourceStatus: {
      /** Format: uint64 */
      age_seconds?: number | null;
      cache?: components["schemas"]["CacheState"] | null;
      source: string;
      status: components["schemas"]["SourceAvailability"];
    };
    /** @description Source discovery for the authenticated principal. */
    Status: {
      cache: components["schemas"]["CachePolicy"];
      config_manage: boolean;
      sources: components["schemas"]["SourceInfo"][];
      version: string;
    };
    /** @description Personal feed issuance and bounded rolling calendar windows. */
    SubscriptionPolicy: {
      /** @default false */
      enabled: boolean;
      /**
       * Format: uint32
       * @default 60
       */
      future_days: number;
      /** @default null */
      master_key: components["schemas"]["Credential"];
      /**
       * Format: uint
       * @default 4096
       */
      max_subjects: number;
      /**
       * Format: uint32
       * @default 7
       */
      past_days: number;
    };
    /** @description One aggregate counter row, without arguments or credentials. */
    ToolStat: {
      /** Format: uint64 */
      calls: number;
      /** Format: uint64 */
      duration_ms: number;
      /** Format: uint64 */
      last_duration_ms: number;
      last_used: string;
      outcome: string;
      tool: string;
    };
    /** @description Account-scoped persistent command counters. */
    ToolStatsPage: {
      client: string;
      items: components["schemas"]["ToolStat"][];
    };
    /** @description Fresh per-account visibility of one registered command. */
    ToolVisibility: {
      available: boolean;
      description: string;
      name: string;
    };
  };
  responses: never;
  parameters: never;
  requestBodies: never;
  headers: never;
  pathItems: never;
}
export type $defs = Record<string, never>;
export type operations = Record<string, never>;

export type Components = components;
