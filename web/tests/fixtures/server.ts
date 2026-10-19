import {
  createServer,
  type IncomingMessage,
  type ServerResponse,
} from "node:http";

import { catalog, findMedia, nativeMedia } from "./media.ts";
import "./seerr.ts";
import "./monitoring.ts";
import "./plex.ts";

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

let nextCommand = 1;
const commands = new Map<number, Record<string, unknown>>();

type NativeKind = "sonarr" | "radarr";

async function submit(request: IncomingMessage) {
  let text = "";

  for await (const chunk of request) {
    text += String(chunk);
  }

  const body = JSON.parse(text) as Record<string, unknown>;
  const id = nextCommand++;
  const command = { body, id, name: body.name, status: "queued" };

  commands.set(id, command);

  return command;
}

const qualityProfiles = [
  {
    id: 1,
    items: [
      {
        allowed: true,
        quality: { id: 3, name: "WEBDL-1080p", resolution: 1080 },
      },
    ],
    name: "1080p",
  },
];

async function nativeRequest(
  kind: NativeKind,
  request: IncomingMessage,
  response: ServerResponse,
) {
  response.setHeader("content-type", "application/json");

  if (
    !["GET", "POST"].includes(request.method ?? "") ||
    request.headers["x-api-key"] !== nativeKey
  ) {
    response.writeHead(401).end('{"error":"native authentication required"}');

    return;
  }

  const url = new URL(request.url ?? "/", "http://127.0.0.1");

  if (request.method === "POST") {
    if (url.pathname !== "/api/v3/command") {
      response.writeHead(404).end("{}");

      return;
    }

    response.writeHead(201).end(JSON.stringify(await submit(request)));

    return;
  }

  const data = readNative(kind, url);

  response
    .writeHead(data === undefined ? 404 : 200)
    .end(JSON.stringify(data ?? {}));
}

function readNative(kind: NativeKind, url: URL): unknown {
  switch (url.pathname) {
    case "/api/v3/calendar": {
      return kind === "sonarr" ? [episode] : [movie];
    }

    case "/api/v3/episode": {
      return [1, 2, 3].map((number) => ({
        airDateUtc: "2026-10-12T18:00:00Z",
        episodeNumber: number,
        hasFile: number === 1,
        id: 700 + number,
        monitored: true,
        seasonNumber: 2,
        seriesId: Number(url.searchParams.get("seriesId")),
        title: `The teapot signal ${number}`,
      }));
    }

    case "/api/v3/movie": {
      return library("movie");
    }

    case "/api/v3/qualityprofile": {
      return qualityProfiles;
    }

    case "/api/v3/queue": {
      return {
        page: Number(url.searchParams.get("page") ?? 1),
        pageSize: Number(url.searchParams.get("pageSize") ?? 20),
        records: [],
        totalRecords: 0,
      };
    }

    case "/api/v3/series": {
      return library("tv");
    }

    case "/api/v3/system/status": {
      return { version: kind === "sonarr" ? "4.0.20.3014" : "6.4.4.10685" };
    }

    default: {
      return readNativeDetail(kind, url.pathname);
    }
  }
}

function readNativeDetail(kind: NativeKind, path: string): unknown {
  const command = /^\/api\/v3\/command\/(\d+)$/.exec(path);

  if (command) {
    const value = commands.get(Number(command[1]));

    return value ? { ...value, status: "completed" } : undefined;
  }

  const detail = /^\/api\/v3\/(series|movie)\/(\d+)$/.exec(path);

  if (!detail) {
    return [];
  }

  const mediaType = kind === "sonarr" ? "tv" : "movie";

  return library(mediaType).find((item) => item.id === Number(detail[2]));
}

for (const [kind, port] of [
  ["sonarr", 8989],
  ["radarr", 7878],
] as const) {
  createServer((request, response) => {
    void nativeRequest(kind, request, response).catch(() =>
      response.writeHead(400).end("{}"),
    );
  }).listen(port, "0.0.0.0");
}
