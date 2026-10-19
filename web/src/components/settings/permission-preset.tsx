import type { SettingsDocument } from "@/lib/settings";

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  matchingPermissionPreset,
  permissionOptions,
  permissionPresets,
} from "@/lib/permissions";

export function PermissionPreset({
  adminBlocked,
  onChange,
  original,
  selected,
  sources,
}: Readonly<{
  adminBlocked: boolean;
  sources: SettingsDocument["sources"];
  selected: string[];
  original: readonly string[];
  onChange: (value: string) => void;
}>) {
  const presets = permissionPresets(sources);
  const current = matchingPermissionPreset(selected, sources);
  const removed = original.filter((grant) => !selected.includes(grant));
  const options = permissionOptions(sources);

  return (
    <div className="grid gap-2">
      <Select
        value={current?.id ?? "custom"}
        onValueChange={(id) => {
          const preset = presets.find((entry) => entry.id === id);

          if (preset && !(adminBlocked && preset.id === "admin")) {
            onChange(preset.grants.join(", "));
          }
        }}
      >
        <SelectTrigger
          aria-label="Permission preset"
          aria-describedby="permission-preset-description permission-preset-hint"
          className="w-full"
        >
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="custom" disabled>
            Custom permissions
          </SelectItem>
          {presets.map((preset) => (
            <SelectItem
              key={preset.id}
              value={preset.id}
              disabled={adminBlocked && preset.id === "admin"}
            >
              {preset.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <p
        id="permission-preset-description"
        className="text-sm text-muted-foreground"
      >
        {current?.description ??
          "Choose Media assistant for full MCP media access, or customize individual permissions below."}
      </p>
      <p id="permission-preset-hint" className="text-xs text-muted-foreground">
        A preset replaces selected permissions. Save to apply.
        {adminBlocked
          ? " Create a separate client without linked accounts to grant administrator access."
          : ""}
      </p>
      {removed.length > 0 && (
        <p className="text-xs text-muted-foreground">
          On save, removes {removed.length}{" "}
          {removed.length === 1 ? "permission" : "permissions"}:{" "}
          {removed
            .slice(0, 3)
            .map(
              (grant) =>
                options.find((option) => option.value === grant)?.label ??
                grant,
            )
            .join(", ")}
          {removed.length > 3 ? ` and ${removed.length - 3} more` : ""}.
        </p>
      )}
    </div>
  );
}
