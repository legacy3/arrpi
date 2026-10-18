"use client";

import { useMutation } from "@tanstack/react-query";
import { Check, Copy } from "lucide-react";

import { Notice } from "@/components/notice";
import { Button } from "@/components/ui/button";

export function CopyText({
  label,
  value,
}: Readonly<{ label: string; value: string }>) {
  const copy = useMutation({
    mutationFn: async () => navigator.clipboard.writeText(value),
    retry: false,
  });

  return (
    <div className="grid gap-2">
      <Button
        type="button"
        variant="outline"
        size="sm"
        disabled={!value || copy.isPending}
        onClick={() => copy.mutate()}
      >
        {copy.isSuccess ? (
          <Check className="size-4" />
        ) : (
          <Copy className="size-4" />
        )}
        {copy.isSuccess ? "Copied" : label}
      </Button>
      {copy.error && (
        <Notice>Could not copy. Select the key and copy it manually.</Notice>
      )}
    </div>
  );
}
