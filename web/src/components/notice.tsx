import type { ReactNode } from "react";

import { CircleAlert, Info } from "lucide-react";

import { Alert, AlertDescription } from "@/components/ui/alert";

export function Notice({
  children,
  tone = "error",
}: Readonly<{ children: ReactNode; tone?: "error" | "info" }>) {
  const Component = tone === "error" ? CircleAlert : Info;

  return (
    <Alert
      role={tone === "error" ? "alert" : undefined}
      variant={tone === "error" ? "destructive" : "default"}
      className={
        tone === "error"
          ? "border-destructive/20 bg-destructive/5"
          : "border-primary/20 bg-primary/5"
      }
    >
      <Component className="size-4" />
      <AlertDescription className="break-words">{children}</AlertDescription>
    </Alert>
  );
}
