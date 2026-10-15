import { createServer } from "node:http";

import type { Badge } from "../../../third_party/native-types/bazarr-1.6.2-api";
import type { StatsV2Response } from "../../../third_party/native-types/cleanuparr-2.10.10-apiv2";
import type {
  StorageCleanupTotals,
  StorageCollectionSummary,
} from "../../../third_party/native-types/maintainerr-3.30.0-api";

import badges from "./native/bazarr-1.6.2-badges.json" with { type: "json" };
import stats from "./native/cleanuparr-2.10.10-stats.json" with { type: "json" };
import storage from "./native/maintainerr-3.30.0-storage.json" with { type: "json" };
import queue from "./native/sabnzbd-5.1.3-queue.json" with { type: "json" };

const key = "fake-upstream-api-key";
const documents: Record<string, unknown> = {
  "/api": queue,
  "/api/badges": badges satisfies Badge,
  "/api/storage-metrics": {
    ...storage,
    cleanupTotals: storage.cleanupTotals satisfies StorageCleanupTotals,
    collectionSummary:
      storage.collectionSummary satisfies StorageCollectionSummary,
  },
  "/api/v2/stats": stats satisfies StatsV2Response,
};

createServer((request, response) => {
  let body = "";

  request.on("data", (chunk: Buffer) => {
    body += chunk.toString();
  });
  request.on("end", () => {
    const path = new URL(request.url ?? "/", "http://127.0.0.1").pathname;
    const sab = path === "/api";
    const authorized = sab
      ? request.method === "POST" &&
        new URLSearchParams(body).get("apikey") === key
      : request.method === "GET" &&
        (path === "/api/storage-metrics" ||
          request.headers["x-api-key"] === key);

    response.setHeader("content-type", "application/json");

    if (!authorized) {
      response.writeHead(401).end('{"error":"native authentication required"}');

      return;
    }

    if (!Object.hasOwn(documents, path)) {
      response.writeHead(404).end("{}");

      return;
    }

    response.end(JSON.stringify(documents[path]));
  });
}).listen(8080, "0.0.0.0");
