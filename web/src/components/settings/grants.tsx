import { ShieldCheck } from "lucide-react";
import { useMemo, useState } from "react";

import type { SettingsDocument } from "@/lib/settings";

import { SearchField } from "@/components/search-field";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from "@/components/ui/accordion";
import { Checkbox } from "@/components/ui/checkbox";
import { useFuzzySearch } from "@/hooks/use-fuzzy-search";
import {
  parsePermissions,
  permissionOptions,
  permissionPresets,
} from "@/lib/permissions";

import { PermissionPreset } from "./permission-preset";

const SEARCH_KEYS = ["label", "value", "group"] as const;

export function Grants({
  adminBlocked = false,
  linked,
  onChange,
  original = [],
  sources,
  value,
}: Readonly<{
  adminBlocked?: boolean;
  linked: boolean;
  original?: readonly string[];
  sources: SettingsDocument["sources"];
  value: string;
  onChange: (value: string) => void;
}>) {
  const [search, setSearch] = useState("");
  const selected = parsePermissions(value);
  const administrator = selected.includes("admin");
  const options = useMemo(() => permissionOptions(sources), [sources]);
  const indexed = useMemo(
    () => [
      ...options,
      ...parsePermissions(value)
        .filter((grant) => !options.some((option) => option.value === grant))
        .map((value) => ({
          area: "other",
          group: "Other grants",
          label: value,
          value,
        })),
    ],
    [options, value],
  );
  const matches = useFuzzySearch({
    items: indexed,
    keys: SEARCH_KEYS,
    query: search,
  });
  const groups = Map.groupBy(matches, (option) => option.group);
  const assistant = permissionPresets(sources).find(
    (preset) => preset.id === "assistant",
  );
  const mediaGrants = assistant?.grants ?? [];
  const missing = mediaGrants.filter(
    (grant) => !administrator && !selected.includes(grant),
  );
  const fullMedia = missing.length === 0;

  let summary = `${mediaGrants.length - missing.length} of ${mediaGrants.length} media permissions selected`;

  if (fullMedia) {
    summary = "All media permissions selected";
  }

  if (administrator) {
    summary = "All ArrPi operations allowed";
  }

  return (
    <div role="group" aria-label="Permissions" className="grid gap-3">
      <PermissionPreset
        sources={sources}
        selected={selected}
        original={original}
        onChange={onChange}
        adminBlocked={adminBlocked}
      />
      <div className="grid gap-2 rounded-lg border bg-muted/20 p-3 text-sm">
        <p aria-live="polite" className="flex items-center gap-2 font-medium">
          <ShieldCheck className="size-4 shrink-0 text-primary" />
          {summary}
        </p>
        <p className="text-xs leading-relaxed text-muted-foreground">
          {linked
            ? "Seerr controls approvals and quotas. Enable Advanced requests for this user in Seerr to choose profiles. Direct API and administration permissions are optional for MCP."
            : "Link a requester account above to use Seerr searches, requests and issues. Library and monitoring tools work without one."}
        </p>
      </div>
      <Accordion type="multiple">
        <AccordionItem value="customize">
          <AccordionTrigger>Customize permissions</AccordionTrigger>
          <AccordionContent className="grid gap-3">
            <SearchField
              label="Search permissions"
              value={search}
              onChange={setSearch}
            />
            <div className="max-h-96 overflow-y-auto rounded-lg border p-3 [contain:layout_paint]">
              {matches.length === 0 && (
                <p className="text-sm text-muted-foreground">
                  No matching permissions.
                </p>
              )}
              {[...groups].map(([group, rows]) => (
                <fieldset key={group} className="mb-4 last:mb-0">
                  <legend className="mb-2 text-sm font-semibold">
                    {group}
                  </legend>
                  {rows[0]?.area === "native" && (
                    <p className="mb-2 text-xs text-muted-foreground">
                      Optional native API access for scripts. MCP media tools do
                      not need this.
                    </p>
                  )}
                  <div className="grid gap-2 sm:grid-cols-2">
                    {rows.map((option) => (
                      <label
                        key={option.value}
                        className="flex min-h-8 cursor-pointer items-start gap-2 rounded-md px-2 py-1.5 text-sm hover:bg-muted/50 has-disabled:cursor-default has-disabled:opacity-70"
                      >
                        <Checkbox
                          className="mt-0.5"
                          checked={
                            administrator || selected.includes(option.value)
                          }
                          disabled={
                            (administrator && option.value !== "admin") ||
                            (adminBlocked &&
                              ["admin", "config:manage"].includes(option.value))
                          }
                          onCheckedChange={(checked) =>
                            onChange(
                              (checked
                                ? [...new Set([...selected, option.value])]
                                : selected.filter(
                                    (grant) => grant !== option.value,
                                  )
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
            {administrator && (
              <p className="text-xs text-muted-foreground">
                Administrator includes every permission shown. Turn off full
                administrator access to customize individual permissions.
              </p>
            )}
          </AccordionContent>
        </AccordionItem>
        <AccordionItem value="raw-grants">
          <AccordionTrigger className="text-muted-foreground">
            Raw grants
          </AccordionTrigger>
          <AccordionContent>
            <p className="mt-2 break-words font-mono text-xs">
              {selected.join(", ") || "No permissions selected"}
            </p>
          </AccordionContent>
        </AccordionItem>
      </Accordion>
    </div>
  );
}
