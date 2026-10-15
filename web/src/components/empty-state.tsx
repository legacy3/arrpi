import type { LucideIcon } from "lucide-react";

import {
  Empty,
  EmptyHeader,
  EmptyMedia,
  EmptyTitle,
} from "@/components/ui/empty";

export function EmptyState({
  icon: Component,
  loading,
  title,
}: Readonly<{ icon: LucideIcon; title: string; loading?: boolean }>) {
  return (
    <Empty className="py-14" role={loading ? "status" : undefined}>
      <EmptyHeader>
        <EmptyMedia variant="icon">
          <Component className="size-4" />
        </EmptyMedia>
        <EmptyTitle>
          <h2>{title}</h2>
        </EmptyTitle>
      </EmptyHeader>
    </Empty>
  );
}
