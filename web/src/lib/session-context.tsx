"use client";

import { createContext, use } from "react";

import type { Session } from "@/lib/session";

export const SessionContext = createContext<Session | null>(null);

export function useSession() {
  const session = use(SessionContext);

  if (!session) {
    throw new Error("No active ArrPi session");
  }

  return session;
}
