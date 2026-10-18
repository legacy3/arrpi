"use client";

import { useQuery } from "@tanstack/react-query";
import { CircleAlert, CircleCheck, Clock3, RefreshCw } from "lucide-react";

import type { Api } from "@/lib/api";

import { Notice } from "@/components/notice";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { outcomeLabel } from "@/lib/presentation";

export type CommandTarget = {
  source: string;
  command_id: number;
  title?: string;
};
type Progress = {
  command: string;
  status: string;
  finished: boolean;
  successful: boolean;
};

export function CommandProgress({
  api,
  onClose,
  target,
}: Readonly<{ api: Api; target: CommandTarget | null; onClose: () => void }>) {
  const query = useQuery({
    enabled: Boolean(target),
    queryFn: async () =>
      (
        await api.command<Progress>("media_command_status", {
          command_id: target?.command_id,
          source: target?.source,
        })
      ).data,
    queryKey: ["command-progress", target?.source, target?.command_id],
    retry: false,
    staleTime: 0,
  });
  const data = query.data;
  let Icon = Clock3;

  if (data?.finished) {
    Icon = data.successful ? CircleCheck : CircleAlert;
  }

  return (
    <Dialog
      open={Boolean(target)}
      onOpenChange={(open) => {
        if (!open) {
          onClose();
        }
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{target?.title ?? "Command progress"}</DialogTitle>
          <DialogDescription>
            {target?.source} command #{target?.command_id}
          </DialogDescription>
        </DialogHeader>
        {query.error ? (
          <Notice>{query.error.message}</Notice>
        ) : (
          <div role="status" className="flex items-center gap-2">
            <Icon className="size-5" />
            {data ? outcomeLabel(data.status) : "Checking progress..."}
          </div>
        )}
        {data?.command && (
          <p className="text-sm text-muted-foreground">
            {data.command.includes("Refresh")
              ? "Metadata refresh and file scan"
              : "Automatic release search"}
          </p>
        )}
        <p className="text-sm text-muted-foreground">
          Command completion does not mean a download has finished.
        </p>
        <div className="flex justify-end gap-2">
          <Button
            variant="outline"
            disabled={query.isFetching}
            onClick={() => void query.refetch()}
          >
            <RefreshCw className="size-4" />
            Refresh
          </Button>
          <Button onClick={onClose}>Close</Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

export function commandReceipt(value: unknown): CommandTarget | null {
  if (
    !value ||
    typeof value !== "object" ||
    !("source" in value) ||
    !("command_id" in value) ||
    typeof value.source !== "string" ||
    typeof value.command_id !== "number" ||
    !Number.isSafeInteger(value.command_id) ||
    value.command_id < 1
  ) {
    return null;
  }

  return {
    command_id: value.command_id,
    source: value.source,
    ...("title" in value && typeof value.title === "string"
      ? { title: value.title }
      : {}),
  };
}
