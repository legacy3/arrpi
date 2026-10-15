import "server-only";

import { coreOrigin } from "../../runtime/core-origin";

export function coreUrl(path: string) {
  if (!path.startsWith("/") || path.startsWith("//") || path.includes("\\")) {
    throw new Error("Invalid ArrPi core path");
  }

  return new URL(path, coreOrigin(process.env.ARRPI_CORE_ORIGIN));
}
