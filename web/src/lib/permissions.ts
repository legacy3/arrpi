import type { SettingsDocument } from "@/lib/settings";

export type PermissionOption = {
  area: Area;
  write: boolean;
  group: string;
  label: string;
  value: string;
};
type Area = "media" | "native" | "tracking" | "server";
type Sources = SettingsDocument["sources"];

const requestReads = [
  ["catalog:view", "Browse request catalog"],
  ["requests:view", "View own requests"],
  ["issues:view", "View issues"],
] as const;
const requestWrites = [
  ["requests:create", "Request media"],
  ["requests:cancel", "Cancel own pending requests"],
  ["issues:create", "Report and close own issues"],
] as const;
const libraryReads = [
  ["library-view", "Browse library and episode details"],
  ["calendar-view", "Read release calendar"],
  ["library-summary", "Read library totals"],
  ["queue-summary", "Read download queue totals"],
  ["status-view", "Read service health"],
] as const;
const maintenance = [
  ["library-refresh", "Refresh metadata and scan files"],
  ["library-search", "Search for missing releases"],
] as const;
const native = [
  ["series", "Read native TV data"],
  ["movie", "Read native movie data"],
  ["queue", "Read native queue"],
  ["health", "Read native health issues"],
  ["qualityprofile", "Read quality profiles"],
  ["calendar", "Read native calendar"],
] as const;

export function matchingPermissionPreset(
  permissions: readonly string[],
  sources: Sources,
) {
  const selected = new Set(permissions);

  return permissionPresets(sources).find((preset) =>
    preset.id === "admin"
      ? selected.has("admin")
      : preset.grants.length === selected.size &&
        preset.grants.every((grant) => selected.has(grant)),
  );
}

export function parsePermissions(value: string): string[] {
  return [
    ...new Set(
      value
        .split(",")
        .map((item) => item.trim())
        .filter(Boolean),
    ),
  ];
}

export function permissionOptions(sources: Sources): PermissionOption[] {
  return [
    ...options("Requests and issues", requestReads, "media"),
    ...options("Requests and issues", requestWrites, "media", undefined, true),
    ...Object.entries(sources).flatMap(([name, source]) => {
      if (["radarr", "sonarr"].includes(source.kind)) {
        return [
          ...options(`${name}: media tools`, libraryReads, "media", name),
          ...options(`${name}: media tools`, maintenance, "media", name, true),
          ...options(
            `${name}: direct API reads`,
            native.filter(([operation]) =>
              source.kind === "sonarr"
                ? operation !== "movie"
                : operation !== "series",
            ),
            "native",
            name,
          ),
        ];
      }

      return [
        ...options(
          `${name}: media tools`,
          [
            ["status-view", "Read service health"],
            ...(source.kind === "plex"
              ? [["ratings-view", "Read Plex community ratings"]]
              : []),
          ],
          "media",
          name,
        ),
        ...(source.kind === "crosswatch"
          ? options(
              "Personal playback tracking",
              [
                ["tracking-view", "View personal tracking"],
                ["tracking-manage", "Manage personal tracking"],
              ],
              "tracking",
              name,
            )
          : []),
      ];
    }),
    ...options(
      "Subscriptions and diagnostics",
      [
        ["metrics", "Read metrics"],
        ["feeds:issue", "Issue personal calendar subscriptions"],
      ],
      "server",
    ),
    ...options(
      "Server administration",
      [
        ["config:manage", "Manage ArrPi settings"],
        ["admin", "Full administrator access"],
      ],
      "server",
    ),
  ];
}

export function permissionPresets(sources: Sources) {
  const all = permissionOptions(sources);
  const mediaReads = all.filter(
    (option) => option.area === "media" && !option.write,
  );
  const assistant = all.filter((option) => option.area === "media");

  return [
    {
      description:
        "Browse libraries, ratings, calendars, requests and service health. No changes or downloads.",
      grants: mediaReads.map(({ value }) => value),
      id: "viewer",
      label: "Read only",
    },
    {
      description:
        "All media tools: browse and request media, manage own issues, refresh metadata and search for missing releases.",
      grants: assistant.map(({ value }) => value),
      id: "assistant",
      label: "Media assistant",
    },
    {
      description:
        "Read-only media access plus native API reads and metrics for dashboards and scripts.",
      grants: [
        ...mediaReads,
        ...all.filter(
          (option) => option.area === "native" || option.value === "metrics",
        ),
      ].map(({ value }) => value),
      id: "api",
      label: "API reader",
    },
    {
      description:
        "Full ArrPi access, including settings and client keys. Use a separate client without a requester account.",
      grants: ["admin"],
      id: "admin",
      label: "Administrator",
    },
  ];
}

export function permissionProfile(
  permissions: readonly string[],
  sources: Sources,
) {
  return (
    matchingPermissionPreset(permissions, sources)?.label ?? "Custom access"
  );
}

function options(
  group: string,
  entries: readonly (readonly string[])[],
  area: Area,
  source?: string,
  write = false,
): PermissionOption[] {
  return entries.map(([value = "", label = ""]) => ({
    area,
    group,
    label,
    value: source ? `${source}:${value}` : value,
    write,
  }));
}
