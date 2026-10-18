import { useMemo, useState } from "react";

import type { SettingsDocument } from "@/lib/settings";

import { SearchField } from "@/components/search-field";
import { Checkbox } from "@/components/ui/checkbox";
import { useFuzzySearch } from "@/hooks/use-fuzzy-search";
const SEARCH_KEYS = ["label", "value", "group"] as const;
const general = [
  ["catalog:view", "Browse request catalog"],
  ["requests:view", "View own requests"],
  ["requests:create", "Request media"],
  ["requests:cancel", "Cancel own pending requests"],
  ["issues:view", "View issues"],
  ["issues:create", "Report and close own issues"],
  ["config:manage", "Manage ArrPi settings"],
  ["admin", "Full administrator access"],
  ["metrics", "Read metrics"],
  ["feeds:issue", "Issue personal calendar subscriptions"],
];
const library = [
  ["library-view", "Browse library"],
  ["library-refresh", "Refresh metadata and scan files"],
  ["library-search", "Search for releases"],
  ["calendar-view", "Read release calendar"],
  ["library-summary", "Read library totals"],
  ["queue-summary", "Read download queue totals"],
  ["status-view", "Read service health"],
];
const native = [
  ["series", "Read native TV data"],
  ["movie", "Read native movie data"],
  ["queue", "Read native queue"],
  ["health", "Read native health issues"],
  ["qualityprofile", "Read quality profiles"],
  ["calendar", "Read native calendar"],
];

export function Grants({
  onChange,
  sources,
  value,
}: Readonly<{
  sources: SettingsDocument["sources"];
  value: string;
  onChange: (value: string) => void;
}>) {
  const [search, setSearch] = useState("");
  const selected = value
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
  const options = useMemo(
    () => [
      ...general.map(([value, label]) => ({
        group: "General",
        label: label ?? "",
        value: value ?? "",
      })),
      ...Object.entries(sources).flatMap(([name, source]) => {
        const arr = ["radarr", "sonarr"].includes(source.kind);
        const grants = arr
          ? [
              ...library,
              ...native.filter(([operation]) =>
                source.kind === "sonarr"
                  ? operation !== "movie"
                  : operation !== "series",
              ),
            ]
          : [
              ["status-view", "Read service health"],
              ...(source.kind === "crosswatch"
                ? [
                    ["tracking-view", "View personal tracking"],
                    ["tracking-manage", "Manage personal tracking"],
                  ]
                : []),
            ];

        return grants.map(([operation, label]) => ({
          group: name,
          label: label ?? "",
          value: `${name}:${operation}`,
        }));
      }),
    ],
    [sources],
  );
  const indexed = useMemo(() => {
    const retained = value
      .split(",")
      .map((grant) => grant.trim())
      .filter(
        (grant) => grant && !options.some((option) => option.value === grant),
      )
      .map((value) => ({ group: "Other grants", label: value, value }));

    return [...options, ...retained];
  }, [options, value]);
  const matches = useFuzzySearch({
    items: indexed,
    keys: SEARCH_KEYS,
    query: search,
  });
  const groups = Map.groupBy(matches, (option) => option.group);

  return (
    <div role="group" aria-label="Permissions" className="grid gap-3">
      <SearchField
        label="Search permissions"
        value={search}
        onChange={setSearch}
      />
      <div className="max-h-96 overflow-y-auto rounded-lg border p-3 [contain:layout_paint]">
        {[...groups].map(([group, rows]) => (
          <fieldset key={group} className="mb-4 last:mb-0">
            <legend className="mb-2 text-sm font-semibold">{group}</legend>
            <div className="grid gap-2 sm:grid-cols-2">
              {rows.map((option) => (
                <label
                  key={option.value}
                  className="flex items-start gap-2 text-sm"
                >
                  <Checkbox
                    checked={selected.includes(option.value)}
                    onCheckedChange={(checked) =>
                      onChange(
                        (checked
                          ? [...new Set([...selected, option.value])]
                          : selected.filter((grant) => grant !== option.value)
                        ).join(", "),
                      )
                    }
                  />
                  <span>{option.label}</span>
                </label>
              ))}
            </div>
          </fieldset>
        ))}
      </div>
      <details>
        <summary className="cursor-pointer text-sm text-muted-foreground">
          Raw grants
        </summary>
        <p className="mt-2 break-words font-mono text-xs">
          {selected.join(", ") || "No permissions selected"}
        </p>
      </details>
    </div>
  );
}
