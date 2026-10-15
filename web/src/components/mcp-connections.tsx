"use client";

import { useMutation } from "@tanstack/react-query";
import { Copy } from "lucide-react";
import Link from "next/link";

import type { Api } from "@/lib/api";
import type { ConnectionKind } from "@/lib/mcp-connection";
import type { SettingsDocument } from "@/lib/settings";

import { Notice } from "@/components/notice";
import { Panel } from "@/components/panel";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { mcpClients, mcpConnection } from "@/lib/mcp-connection";
import { isRequesterClient } from "@/lib/settings";

export function McpConnections({
  api,
  clients,
  endpoint,
  onSelect,
  ready,
  selected,
}: Readonly<{
  api: Api;
  ready: boolean;
  clients: SettingsDocument["clients"];
  endpoint: string;
  selected: string;
  onSelect: (name: string) => void;
}>) {
  const account = clients.find((value) => value.name === selected);
  const allowed = ready && isRequesterClient(account);
  const copy = useMutation({
    mutationFn: (value: { account: string; kind: ConnectionKind }) =>
      copyConnection(api, value.account, endpoint, value.kind),
  });

  return (
    <Panel
      title="Connect a client"
      detail={
        <Button asChild variant="ghost" size="sm">
          <Link href="/settings/mcp" prefetch={false}>
            MCP settings
          </Link>
        </Button>
      }
    >
      <div className="space-y-4 px-5 pb-5">
        <div className="max-w-md">
          <div className="flex min-w-0 items-center gap-3">
            <Label className="w-14 shrink-0" htmlFor="mcp-account">
              Account
            </Label>
            <select
              id="mcp-account"
              className="min-w-0 max-w-full flex-1 rounded-md border bg-background px-3 py-2 text-sm"
              value={selected}
              disabled={copy.isPending}
              onChange={(event) => {
                copy.reset();
                onSelect(event.target.value);
              }}
            >
              {clients.map((value) => (
                <option key={value.name} value={value.name}>
                  {value.name}
                </option>
              ))}
            </select>
          </div>
        </div>
        <div className="flex flex-wrap gap-2">
          {mcpClients.map(({ id, label }) => (
            <Button
              key={id}
              variant="outline"
              size="sm"
              aria-label={`Copy ${label}`}
              disabled={!allowed || copy.isPending}
              onClick={() => copy.mutate({ account: selected, kind: id })}
            >
              <Copy className="size-3.5" />
              {copy.isSuccess &&
              copy.variables.account === selected &&
              copy.variables.kind === id
                ? "Copied"
                : "Copy"}{" "}
              {label}
            </Button>
          ))}
        </div>
        {copy.error && <Notice>{copy.error.message}</Notice>}
        {ready && !allowed && (
          <Notice tone="info">
            Choose a Plex-bound account without ArrPi admin grants.
          </Notice>
        )}
      </div>
    </Panel>
  );
}

async function copyConnection(
  api: Api,
  account: string,
  endpoint: string,
  kind: ConnectionKind,
) {
  const clipboard = navigator.clipboard;

  if (!clipboard) {
    throw new Error("Copying needs HTTPS or localhost");
  }

  let sourceError: unknown;
  const text = api
    .read<{ token: string }>(
      `/v1/mcp/token?client=${encodeURIComponent(account)}`,
    )
    .then((value) => mcpConnection(kind, endpoint, value.data.token))
    .catch((error: unknown) => {
      sourceError = error;
      throw error;
    });

  try {
    await (typeof ClipboardItem !== "undefined" && clipboard.write
      ? clipboard.write([
          new ClipboardItem({
            "text/plain": text.then(
              (value) => new Blob([value], { type: "text/plain" }),
            ),
          }),
        ])
      : clipboard.writeText(await text));
  } catch (error) {
    throw sourceError ?? error;
  }
}
