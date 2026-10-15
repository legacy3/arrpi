"use client";

import { useSize } from "ahooks";
import { Save } from "lucide-react";
import { useEffect, useRef } from "react";

import { ConfigConflict } from "@/components/config-conflict";
import { Notice } from "@/components/notice";
import { Button } from "@/components/ui/button";

/** One save action for the whole revision-checked document, beside its outcome. */
export function FormActions({
  conflict,
  disabled = false,
  error,
  message,
  pending,
  reload,
}: Readonly<{
  disabled?: boolean;
  pending: boolean;
  conflict: boolean;
  error: Error | null;
  message?: string;
  reload: () => Promise<unknown>;
}>) {
  const actionsRef = useRef<HTMLDivElement>(null);
  const size = useSize(actionsRef);

  useEffect(() => {
    if (!size || !actionsRef.current) {
      return;
    }

    const root = document.documentElement;

    root.style.setProperty(
      "--arrpi-form-actions-height",
      `${Math.ceil(actionsRef.current.getBoundingClientRect().height)}px`,
    );

    return () => {
      root.style.removeProperty("--arrpi-form-actions-height");
    };
  }, [size]);

  return (
    <div
      ref={actionsRef}
      data-slot="form-actions"
      className="sticky bottom-4 z-10 flex flex-wrap items-center justify-end gap-3 rounded-xl border bg-card/95 p-3 shadow-lg backdrop-blur"
    >
      {error && !conflict && (
        <div className="min-w-0 flex-1">
          <Notice>{error.message}</Notice>
        </div>
      )}
      {conflict && (
        <div className="min-w-0 flex-1">
          <ConfigConflict reload={reload} />
        </div>
      )}
      {message && !pending && !error && !conflict && (
        <p role="status" className="mr-auto text-sm text-primary">
          {message}
        </p>
      )}
      <Button type="submit" disabled={pending || disabled}>
        <Save className="size-4" />
        {pending ? "Saving…" : "Save"}
      </Button>
    </div>
  );
}
