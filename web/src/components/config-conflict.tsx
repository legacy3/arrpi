import { Button } from "@/components/ui/button";

export function ConfigConflict({
  reload,
}: Readonly<{ reload: () => Promise<unknown> }>) {
  return (
    <div className="flex flex-wrap items-center gap-3">
      <p className="text-sm text-muted-foreground">
        Settings changed elsewhere. Reload to review the current values before
        saving again.
      </p>
      <Button type="button" variant="outline" onClick={() => void reload()}>
        Reload settings
      </Button>
    </div>
  );
}
