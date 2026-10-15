import type { Components as components } from "@/lib/generated/contracts";

export type SettingsDocument = components["schemas"]["SettingsDocument"];
export type SettingsEnvelope = components["schemas"]["SettingsEnvelope"];

export function hasConfigAccess(permissions: readonly string[]) {
  return permissions.some((permission) =>
    ["admin", "config:manage"].includes(permission),
  );
}

/** Request credentials must never confer configuration access. */
export function isRequesterClient(
  client?: SettingsDocument["clients"][number],
) {
  return (
    Boolean(client?.seerr_user) && !hasConfigAccess(client?.permissions ?? [])
  );
}
