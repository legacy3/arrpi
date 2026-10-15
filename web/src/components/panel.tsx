import type { ReactNode } from "react";

import { Card, CardAction, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "@/lib/utils";

export function Panel({
  children,
  className,
  detail,
  title,
}: Readonly<{
  title?: string;
  detail?: ReactNode;
  className?: string;
  children: ReactNode;
}>) {
  return (
    <Card className={cn("gap-0 py-0", className)}>
      {title && (
        <CardHeader className="px-5 py-5">
          <CardTitle>
            <h2 className="text-sm font-semibold">{title}</h2>
          </CardTitle>
          {detail !== undefined && (
            <CardAction className="text-xs text-muted-foreground">
              {detail}
            </CardAction>
          )}
        </CardHeader>
      )}
      {children}
    </Card>
  );
}
