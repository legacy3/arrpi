"use client";

import {
  Box,
  CalendarDays,
  Code,
  Gauge,
  LogOut,
  Plug,
  RefreshCw,
  Settings2,
} from "lucide-react";
import Link from "next/link";
import { type ReactNode } from "react";

import { Badge } from "@/components/ui/badge";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import { cn } from "@/lib/utils";

export type Tab = "overview" | "calendar" | "explorer" | "settings" | "mcp";

const tabs = [
  { href: "/", icon: Gauge, id: "overview", label: "Overview" },
  { href: "/calendar", icon: CalendarDays, id: "calendar", label: "Calendar" },
  { href: "/explorer", icon: Code, id: "explorer", label: "Explorer" },
  { href: "/mcp", icon: Plug, id: "mcp", label: "MCP" },
  { href: "/settings", icon: Settings2, id: "settings", label: "Settings" },
] as const;

type ShellProps = Readonly<{
  tab: Tab;
  connected: boolean;
  configManage?: boolean;
  version?: string;
  disconnect?: () => void;
  refresh?: () => void;
  refreshing?: boolean;
  children: ReactNode;
}>;

export function requiresConfig(tab: Tab) {
  return ["mcp", "settings"].includes(tab);
}

export function Shell({
  children,
  configManage = false,
  connected,
  disconnect,
  refresh,
  refreshing,
  tab,
  version,
}: ShellProps) {
  const selected = tabs.find((item) => item.id === tab) ?? tabs[0];
  const title = connected ? selected.label : "Connect";

  return (
    <div className="min-h-dvh">
      <aside className="fixed inset-y-0 left-0 hidden w-56 flex-col border-r bg-card/50 md:flex">
        <div className="flex h-16 items-center px-6">
          <Brand />
        </div>
        <Separator />
        <div className="flex-1 overflow-y-auto px-3 py-5">
          <Navigation
            tab={tab}
            connected={connected}
            configManage={configManage}
          />
        </div>
        <div className="border-t p-3">
          <Connection connected={connected} disconnect={disconnect} />
        </div>
      </aside>
      <div className="md:pl-56">
        <header className="sticky top-0 z-20 flex h-16 items-center justify-between gap-4 border-b bg-background/95 px-5 backdrop-blur sm:px-8">
          <div className="md:hidden">
            <Brand />
          </div>
          <Breadcrumb className="hidden md:block">
            <BreadcrumbList>
              <BreadcrumbItem>ArrPi</BreadcrumbItem>
              <BreadcrumbSeparator />
              <BreadcrumbItem>
                <BreadcrumbPage>{title}</BreadcrumbPage>
              </BreadcrumbItem>
            </BreadcrumbList>
          </Breadcrumb>
          <div className="flex items-center gap-3">
            {version && (
              <Badge
                asChild
                variant="outline"
                className="hidden font-mono text-xs text-muted-foreground sm:inline-flex"
              >
                <a
                  href="https://github.com/legacy3/arrpi"
                  target="_blank"
                  rel="noreferrer"
                >
                  v{version}
                </a>
              </Badge>
            )}
            <div className="md:hidden">
              {connected && disconnect && (
                <Button
                  variant="ghost"
                  size="sm"
                  className="text-muted-foreground"
                  onClick={disconnect}
                >
                  <LogOut className="size-4" />
                  <span className="sr-only sm:not-sr-only">Disconnect</span>
                </Button>
              )}
            </div>
          </div>
        </header>
        <div className="border-b px-3 py-2 md:hidden">
          <Navigation
            tab={tab}
            connected={connected}
            configManage={configManage}
            mobile
          />
        </div>
        <main className="mx-auto w-full max-w-7xl space-y-6 px-4 py-5 sm:px-8 sm:py-7">
          <div className="flex items-center justify-between gap-4">
            <h1 className="text-2xl font-semibold tracking-tight sm:text-3xl">
              {title}
            </h1>
            {refresh && (
              <Button
                variant="outline"
                size="sm"
                aria-label="Refresh cache status"
                disabled={refreshing}
                onClick={refresh}
              >
                <RefreshCw
                  className={cn("size-4", refreshing && "animate-spin")}
                />
                Refresh
              </Button>
            )}
          </div>
          {children}
        </main>
      </div>
    </div>
  );
}

export function tabForPath(path: string): Tab {
  if (path.startsWith("/settings/")) {
    return "settings";
  }

  return tabs.find((item) => item.href === path)?.id ?? "overview";
}

function Brand() {
  return (
    <Button
      variant="ghost"
      className="h-auto gap-2.5 px-0 hover:bg-transparent"
      aria-label="ArrPi overview"
      asChild
    >
      <Link href="/" prefetch={false}>
        <span className="flex size-8 items-center justify-center rounded-lg border border-primary/25 bg-primary/10 text-primary">
          <Box className="size-4" />
        </span>
        <span className="text-lg font-semibold tracking-tight">
          Arr<span className="text-primary">Pi</span>
        </span>
      </Link>
    </Button>
  );
}

function Connection({
  connected,
  disconnect,
}: Readonly<{ connected: boolean; disconnect?: () => void }>) {
  return (
    <Button
      variant="outline"
      size="sm"
      disabled={!connected || !disconnect}
      aria-label={
        connected && disconnect ? "Connected — Disconnect" : undefined
      }
      onClick={disconnect}
      className="w-full justify-start gap-2 text-xs text-muted-foreground disabled:opacity-100"
    >
      <span
        className={cn(
          "size-1.5 rounded-full bg-muted-foreground",
          connected && "bg-emerald-400",
        )}
      />
      {connected ? "Connected" : "Not connected"}
      {connected && disconnect && <LogOut className="ml-auto size-3.5" />}
    </Button>
  );
}

function Navigation({
  configManage,
  connected,
  mobile,
  tab,
}: Readonly<{
  connected: boolean;
  mobile?: boolean;
  configManage?: boolean;
  tab: Tab;
}>) {
  return (
    <nav
      aria-label="Main navigation"
      className={cn(
        "grid gap-1",
        mobile && (configManage ? "grid-cols-5" : "grid-cols-3"),
      )}
    >
      {tabs
        .filter((item) => !requiresConfig(item.id) || configManage)
        .map(({ href, icon: Component, id, label }) => {
          const content = (
            <>
              <Component className="size-4" />
              <span>{label}</span>
            </>
          );

          return (
            <Button
              key={id}
              variant="ghost"
              disabled={!connected}
              asChild={connected}
              aria-current={connected && tab === id ? "page" : undefined}
              className={cn(
                "justify-start gap-3 text-muted-foreground",
                mobile &&
                  "h-auto flex-col justify-center gap-1 px-1 py-2 text-[11px]",
                connected &&
                  tab === id &&
                  "bg-primary/10 text-primary hover:bg-primary/15 hover:text-primary",
              )}
            >
              {connected ? (
                <Link href={href} prefetch={false}>
                  {content}
                </Link>
              ) : (
                content
              )}
            </Button>
          );
        })}
    </nav>
  );
}
