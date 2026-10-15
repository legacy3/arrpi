"use client";

import { CalendarDays } from "lucide-react";
import dynamic from "next/dynamic";

import type { Tab } from "@/components/shell";

import { EmptyState } from "@/components/empty-state";
import { Notice } from "@/components/notice";
import { requiresConfig } from "@/components/shell";
import { Card } from "@/components/ui/card";
import { useSession } from "@/lib/session-context";

const Calendar = dynamic(() =>
  import("@/components/calendar").then((value) => value.Calendar),
);
const Explorer = dynamic(() =>
  import("@/components/explorer").then((value) => value.Explorer),
);
const Mcp = dynamic(() =>
  import("@/components/mcp").then((value) => value.Mcp),
);
const Overview = dynamic(() =>
  import("@/components/overview").then((value) => value.Overview),
);
const Settings = dynamic(() =>
  import("@/components/settings").then((value) => value.Settings),
);

export function DashboardPage({ view }: Readonly<{ view: Tab }>) {
  const { api, status } = useSession();

  if (requiresConfig(view) && !status.config_manage) {
    return <Notice tone="info">Configuration access required</Notice>;
  }

  switch (view) {
    case "calendar": {
      return status.sources.some((source) => source.calendar) ? (
        <Calendar api={api} status={status} />
      ) : (
        <Card>
          <EmptyState icon={CalendarDays} title="No calendar access" />
        </Card>
      );
    }

    case "explorer": {
      return <Explorer api={api} status={status} />;
    }

    case "mcp": {
      return <Mcp api={api} />;
    }

    case "overview": {
      return <Overview api={api} status={status} />;
    }

    case "settings": {
      return <Settings api={api} />;
    }
  }
}
