import type { Metadata } from "next";

import { notFound } from "next/navigation";

import { DashboardPage } from "@/components/dashboard-page";

export const metadata: Metadata = { title: "Settings" };

export default async function Page({
  params,
}: Readonly<{ params: Promise<{ section: string }> }>) {
  const { section } = await params;

  if (!["access", "feeds", "mcp", "oauth", "server"].includes(section)) {
    notFound();
  }

  return <DashboardPage view="settings" />;
}
