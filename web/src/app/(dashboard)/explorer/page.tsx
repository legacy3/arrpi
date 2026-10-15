import type { Metadata } from "next";

import { DashboardPage } from "@/components/dashboard-page";

export const metadata: Metadata = { title: "Explorer" };

export default function Page() {
  return <DashboardPage view="explorer" />;
}
