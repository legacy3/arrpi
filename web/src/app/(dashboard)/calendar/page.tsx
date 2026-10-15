import type { Metadata } from "next";

import { DashboardPage } from "@/components/dashboard-page";

export const metadata: Metadata = { title: "Calendar" };

export default function Page() {
  return <DashboardPage view="calendar" />;
}
