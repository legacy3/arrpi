import { createServer } from "node:http";

import { catalog, findMedia, nativeMedia } from "./media.ts";
import "./seerr.ts";
import "./monitoring.ts";

const station = findMedia("tv", 84);
const lantern = findMedia("movie", 99);

if (!station || !lantern) {
  throw new Error("Missing calendar fixture media");
}

const nativeKey = "fake-upstream-api-key";
const episode = {
  airDateUtc: "2026-10-15T21:00:00Z",
  episodeNumber: 7,
  id: 1,
  monitored: true,
  seasonNumber: 2,
  series: nativeMedia(station),
  title: "The missing teapot",
};
const movie = {
  ...nativeMedia(lantern),
  digitalRelease: "2026-10-16",
};
const library = (kind: "movie" | "tv") =>
  catalog
    .filter((item) => item.kind === kind && item.libraryId)
    .map((item) => nativeMedia(item));

for (const [kind, port] of [
  ["sonarr", 8989],
  ["radarr", 7878],
] as const) {
  createServer((request, response) => {
    response.setHeader("content-type", "application/json");

    if (
      request.method !== "GET" ||
      request.headers["x-api-key"] !== nativeKey
    ) {
      response.writeHead(401).end('{"error":"native authentication required"}');

      return;
    }

    const url = new URL(request.url ?? "/", "http://127.0.0.1");
    const path = url.pathname;
    let data: unknown = [];

    if (path.endsWith("/calendar")) {
      data = kind === "sonarr" ? [episode] : [movie];
    }

    if (path.endsWith("/series")) {
      data = library("tv");
    }

    if (path.endsWith("/movie")) {
      data = library("movie");
    }

    if (path.endsWith("/queue")) {
      data = {
        page: Number(url.searchParams.get("page") ?? 1),
        pageSize: Number(url.searchParams.get("pageSize") ?? 20),
        records: [],
        totalRecords: 0,
      };
    }

    response.end(JSON.stringify(data));
  }).listen(port, "0.0.0.0");
}
