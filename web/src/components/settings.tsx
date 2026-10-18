"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useState } from "react";

import type { Api } from "@/lib/api";
import type { SettingsEnvelope } from "@/lib/settings";

import { Notice } from "@/components/notice";
import { Access } from "@/components/settings/access";
import { Connections } from "@/components/settings/connections";
import { Feeds } from "@/components/settings/feeds";
import { General } from "@/components/settings/general";
import { McpSettings } from "@/components/settings/mcp";
import { OAuth } from "@/components/settings/oauth";
import { Server } from "@/components/settings/server";
import { Button } from "@/components/ui/button";
import { useApiQuery } from "@/lib/use-api-query";

const sections = [
  {
    component: General,
    href: "/settings/general",
    id: "general",
    label: "General",
  },
  {
    component: Connections,
    href: "/settings",
    id: "connections",
    label: "Connections",
  },
  {
    component: Access,
    href: "/settings/access",
    id: "access",
    label: "Access",
  },
  { component: Feeds, href: "/settings/feeds", id: "feeds", label: "Feeds" },
  { component: McpSettings, href: "/settings/mcp", id: "mcp", label: "MCP" },
  { component: OAuth, href: "/settings/oauth", id: "oauth", label: "OAuth" },
  {
    component: Server,
    href: "/settings/server",
    id: "server",
    label: "Server",
  },
] as const;

export function Settings({ api }: Readonly<{ api: Api }>) {
  const path = usePathname();
  const query = useApiQuery<SettingsEnvelope>(api, "/v1/config");
  const [message, setMessage] = useState<string>();
  const selected =
    sections.find((section) => section.href === path) ?? sections[1];
  const Active = selected.component;

  return (
    <div className="grid gap-6">
      <nav
        aria-label="Settings sections"
        className="flex flex-wrap gap-1 border-b pb-3"
      >
        {sections.map((section) => (
          <Button
            key={section.id}
            asChild
            variant={selected.id === section.id ? "secondary" : "ghost"}
            size="sm"
          >
            <Link
              href={section.href}
              prefetch={false}
              aria-current={selected.id === section.id ? "page" : undefined}
            >
              {section.label}
            </Link>
          </Button>
        ))}
      </nav>
      {query.error && <Notice>{query.error.message}</Notice>}
      {!query.error && !query.data && (
        <p role="status" className="text-muted-foreground">
          Loading settings…
        </p>
      )}
      {!query.error && query.data && (
        <>
          <Active
            key={
              ["access", "connections", "oauth"].includes(selected.id)
                ? selected.id
                : `${selected.id}:${query.data.data.revision}`
            }
            api={api}
            document={query.data.data}
            message={message}
            onEdit={() => setMessage(undefined)}
            onSaved={(restart) =>
              setMessage(
                restart
                  ? "Saved. Restart ArrPi to apply connection or listener changes."
                  : "Saved and applied.",
              )
            }
          />
        </>
      )}
    </div>
  );
}
