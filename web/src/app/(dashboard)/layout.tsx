import type { ReactNode } from "react";

import { headers } from "next/headers";

import { Dashboard } from "@/components/dashboard";
import { ViewState } from "@/components/view-state";

export default async function DashboardLayout({
  children,
}: Readonly<{ children: ReactNode }>) {
  const proxyAuth = (await headers()).get("x-arrpi-web-auth") === "proxy";

  return (
    <ViewState>
      <Dashboard proxyAuth={proxyAuth}>{children}</Dashboard>
    </ViewState>
  );
}
