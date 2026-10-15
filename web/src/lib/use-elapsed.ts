"use client";

import { useInterval } from "ahooks";
import { useState } from "react";

export function useElapsed(receivedAt?: number) {
  const [now, setNow] = useState(Date.now);

  useInterval(
    () => setNow(Date.now()),
    receivedAt === undefined ? undefined : 15_000,
  );

  return receivedAt === undefined
    ? 0
    : Math.max(0, Math.floor((now - receivedAt) / 1000));
}
