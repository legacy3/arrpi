import type { LucideIcon } from "lucide-react";

import { cn } from "@/lib/utils";

export function Icon({
  className,
  icon: Component,
}: Readonly<{
  icon: LucideIcon;
  className?: string;
}>) {
  return (
    <Component
      aria-hidden="true"
      focusable="false"
      className={cn("size-4 shrink-0", className)}
    />
  );
}
