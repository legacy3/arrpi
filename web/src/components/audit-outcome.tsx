import { CircleAlert, CircleCheck, Clock3 } from "lucide-react";

import { errorText, outcomeLabel } from "@/lib/presentation";
import { cn } from "@/lib/utils";

const completed = new Set([
  "already_available",
  "already_requested",
  "approved",
  "available",
  "canceled",
  "cancelled",
  "closed",
  "completed",
  "ok",
  "resolved",
]);
const pending = new Set(["open", "pending", "queued", "running", "started"]);

export function AuditOutcome({ outcome }: Readonly<{ outcome: string }>) {
  const success = completed.has(outcome);
  const waiting = pending.has(outcome);
  let Icon = CircleAlert;
  let tone = "text-amber-300";

  if (success) {
    Icon = CircleCheck;
    tone = "text-emerald-300";
  } else if (waiting) {
    Icon = Clock3;
    tone = "text-muted-foreground";
  }

  return (
    <span
      data-slot="audit-outcome"
      title={errorText(outcome)}
      className="inline-flex min-w-0 items-start justify-end gap-1.5 text-xs leading-5"
    >
      <Icon
        aria-hidden="true"
        className={cn(
          "mt-0.5 hidden size-3.5 shrink-0 min-[360px]:block",
          tone,
        )}
      />
      <span className="min-w-0 text-left">{outcomeLabel(outcome)}</span>
    </span>
  );
}
