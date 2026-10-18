import {
  createServer,
  type IncomingMessage,
  type ServerResponse,
} from "node:http";

import { catalog, findMedia, type Kind, type Media } from "./media.ts";

type Issue = {
  createdBy: { id: number };
  id: number;
  issueType: number;
  media: { mediaType: Kind; tmdbId: number };
  problemEpisode?: number;
  problemSeason?: number;
  status: number;
};

type Receipt = {
  id: number;
  is4k: false;
  media: {
    mediaType: Kind;
    releaseDate: string;
    title: string;
    tmdbId: number;
  };
  createdAt: string;
  updatedAt: string;
  requestedBy: { id: number };
  seasons: { seasonNumber: number; status: number }[];
  status: number;
  type: "movie" | "tv";
};
const issues = new Map<number, Issue>();
const receipts = new Map<number, Receipt>();
let nextId = 1;
// Seerr v3.5.0 Permission.AUTO_APPROVE. Actor 1 intentionally requires approval.
const autoApprove = 128;

function availability(requests: Receipt[]) {
  if (requests.some((receipt) => receipt.status === 2)) {
    return 3;
  }

  return requests.length > 0 ? 2 : 1;
}

async function body(request: IncomingMessage): Promise<unknown> {
  const chunks: Buffer[] = [];

  for await (const chunk of request) {
    chunks.push(Buffer.from(chunk as Uint8Array));
  }

  return JSON.parse(Buffer.concat(chunks).toString()) as unknown;
}

function cancelRequest(response: ServerResponse, actor: number, id: number) {
  const receipt = receipts.get(id);

  if (!receipt || receipt.requestedBy.id !== actor || receipt.status !== 1) {
    response.writeHead(403).end('{"error":"request not owned or pending"}');

    return;
  }

  receipts.delete(id);
  response.writeHead(204).end();
}

async function createIssue(
  request: IncomingMessage,
  response: ServerResponse,
  actor: number,
) {
  const value = (await body(request)) as Record<string, unknown>;
  const item = catalog.find(
    (candidate) => candidate.id + 100_000 === value.mediaId,
  );

  if (
    !item ||
    typeof value.message !== "string" ||
    !value.message.trim() ||
    typeof value.issueType !== "number" ||
    ![1, 2, 3, 4].includes(value.issueType) ||
    issues.size >= 128
  ) {
    response.writeHead(400).end('{"error":"invalid fixture issue"}');

    return;
  }

  const issue: Issue = {
    createdBy: { id: actor },
    id: nextId++,
    issueType: value.issueType,
    media: { mediaType: item.kind, tmdbId: item.id },
    status: 1,
    ...(typeof value.problemSeason === "number"
      ? { problemSeason: value.problemSeason }
      : {}),
    ...(typeof value.problemEpisode === "number"
      ? { problemEpisode: value.problemEpisode }
      : {}),
  };

  issues.set(issue.id, issue);
  response.writeHead(201).end(JSON.stringify(issue));
}

async function createRequest(
  request: IncomingMessage,
  response: ServerResponse,
  actor: number,
) {
  try {
    const value = await body(request);

    if (
      !isRequest(value) ||
      !findMedia(value.mediaType, value.mediaId) ||
      receipts.size >= 128
    ) {
      response.writeHead(400).end('{"error":"unsafe native request"}');

      return;
    }

    const item = findMedia(value.mediaType, value.mediaId);

    if (!item) {
      throw new Error("Missing request fixture");
    }

    const status = (permissions(actor) & autoApprove) === 0 ? 1 : 2;
    const receipt: Receipt = {
      createdAt: new Date().toISOString(),
      id: nextId++,
      is4k: false,
      media: {
        mediaType: item.kind,
        releaseDate: `${item.year}-01-01`,
        title: item.title,
        tmdbId: item.id,
      },
      requestedBy: { id: actor },
      seasons: (value.seasons ?? []).map((seasonNumber) => ({
        seasonNumber,
        status,
      })),
      status,
      type: value.mediaType,
      updatedAt: new Date().toISOString(),
    };

    receipts.set(receipt.id, receipt);
    response.writeHead(201).end(JSON.stringify(receipt));
  } catch {
    response.writeHead(400).end('{"error":"invalid fixture body"}');
  }
}

function isRequest(value: unknown): value is {
  is4k: false;
  mediaId: number;
  mediaType: "movie" | "tv";
  seasons?: number[];
} {
  if (typeof value !== "object" || value === null) {
    return false;
  }

  const item = value as Record<string, unknown>;

  return (
    item.is4k === false &&
    typeof item.mediaId === "number" &&
    Number.isSafeInteger(item.mediaId) &&
    item.mediaId > 0 &&
    (item.mediaType === "movie" || item.mediaType === "tv") &&
    !Object.hasOwn(item, "userId") &&
    !Object.hasOwn(item, "ignoreQuota") &&
    !Object.hasOwn(item, "tags") &&
    (item.seasons === undefined ||
      (Array.isArray(item.seasons) &&
        item.seasons.every(
          (season) => Number.isSafeInteger(season) && season > 0,
        )))
  );
}

function metadata(item: Media) {
  const requested = [...receipts.values()].filter(
    (receipt) => receipt.type === item.kind && receipt.media.tmdbId === item.id,
  );
  const seasons = (item.seasons ?? []).map((episodeCount, index) => {
    const status = item.availableSeasons?.includes(index + 1)
      ? 5
      : availability(
          requested.filter((receipt) =>
            receipt.seasons.some((season) => season.seasonNumber === index + 1),
          ),
        );

    return { episodeCount, seasonNumber: index + 1, status };
  });
  const fullyAvailable =
    item.kind === "movie"
      ? Boolean(item.libraryId)
      : seasons.length > 0 && seasons.every((season) => season.status === 5);
  let status = availability(requested);

  if (fullyAvailable) {
    status = 5;
  } else if (item.availableSeasons && item.availableSeasons.length > 0) {
    status = 4;
  }

  return {
    id: item.id,
    ...(item.kind === "movie" ? { title: item.title } : { name: item.title }),
    externalIds: { imdbId: `tt999000${item.id}`, tvdbId: 999_000 + item.id },
    firstAirDate: `${item.year}-01-01`,
    genres: item.genres.map((name, index) => ({ id: index + 1, name })),
    mediaInfo: {
      id: item.id + 100_000,
      mediaUrl: item.libraryId
        ? `https://watch.example.test/web/index.html#!/server/fictional/details?key=%2Flibrary%2Fmetadata%2F${item.id}`
        : undefined,
      requests: requested,
      seasons,
      status,
      updatedAt: "2026-10-14T00:00:00Z",
    },
    mediaType: item.kind,
    overview: item.overview,
    releaseDate: `${item.year}-01-01`,
    runtime: item.runtime,
    seasons,
    voteAverage: item.rating,
  };
}

function page<T>(items: T[], query: URLSearchParams) {
  const skip = Math.max(0, Number(query.get("skip") ?? 0));
  const take = Math.min(100, Math.max(1, Number(query.get("take") ?? 20)));

  return {
    pageInfo: {
      page: 1,
      pages: Math.ceil(items.length / take),
      results: items.length,
    },
    results: items.slice(skip, skip + take),
  };
}

function permissions(actor: number) {
  return 32 | 2_097_152 | 4_194_304 | (actor === 2 ? autoApprove : 0);
}

function read(path: string, actor: number, query: URLSearchParams): unknown {
  if (path === "/api/v1/auth/me") {
    return {
      email: "private-fixture@example.test",
      id: actor,
      permissions: permissions(actor),
      plexId: actor + 100,
    };
  }

  if (path === `/api/v1/user/${actor}/quota`) {
    const quota = { days: 7, limit: 0, restricted: false, used: 0 };

    return { movie: quota, tv: quota };
  }

  if (path === "/api/v1/settings/public") {
    return { applicationUrl: "https://request.example.test" };
  }

  if (path === "/api/v1/search") {
    const queryText = (query.get("query") ?? "").toLowerCase();
    const matches = catalog.filter((item) =>
      item.title.toLowerCase().includes(queryText),
    );

    return {
      page: 1,
      results: matches.map((item) => metadata(item)),
      totalPages: matches.length > 0 ? 1 : 0,
    };
  }

  if (
    path === "/api/v1/request" &&
    query.get("requestedBy") === String(actor)
  ) {
    return page(
      [...receipts.values()].filter(
        (receipt) => receipt.requestedBy.id === actor,
      ),
      query,
    );
  }

  if (path === "/api/v1/issue") {
    return page([...issues.values()], query);
  }

  return readDetail(path);
}

function readDetail(path: string): unknown {
  const media = /^\/api\/v1\/(movie|tv)\/(\d+)$/.exec(path);

  if (media) {
    const item = findMedia(media[1] as Kind, Number(media[2]));

    return item ? metadata(item) : undefined;
  }

  const issue = /^\/api\/v1\/issue\/(\d+)$/.exec(path);

  if (issue) {
    return issues.get(Number(issue[1]));
  }

  const season = /^\/api\/v1\/tv\/(\d+)\/season\/(\d+)$/.exec(path);

  if (season) {
    const count = findMedia("tv", Number(season[1]))?.seasons?.[
      Number(season[2]) - 1
    ];

    return count === undefined
      ? undefined
      : {
          episodes: Array.from({ length: count }, (_, index) => ({
            episodeNumber: index + 1,
          })),
          seasonNumber: Number(season[2]),
        };
  }

  const receipt = /^\/api\/v1\/request\/(\d+)$/.exec(path);

  return receipt ? receipts.get(Number(receipt[1])) : undefined;
}

async function reply(request: IncomingMessage, response: ServerResponse) {
  response.setHeader("content-type", "application/json");
  const url = new URL(request.url ?? "/", "http://127.0.0.1");

  if (request.method === "GET" && url.pathname === "/api/v1/status") {
    response.end(JSON.stringify({ version: "3.5.0" }));

    return;
  }

  if (
    request.method === "GET" &&
    url.pathname === "/api/v1/user" &&
    request.headers["x-api-key"] === "fake-seerr-key"
  ) {
    const users = [
      { displayName: "Fixture owner", id: 1, plexId: 101 },
      { displayName: "Fixture friend", id: 2, plexId: 102 },
    ];
    const skip = Number(url.searchParams.get("skip") ?? 0);
    const take = Number(url.searchParams.get("take") ?? 100);

    response.end(
      JSON.stringify({
        pageInfo: { results: users.length },
        results: users.slice(skip, skip + take),
      }),
    );

    return;
  }

  const actor = Number(request.headers["x-api-user"]);

  if (
    request.method === "GET" &&
    url.pathname === "/api/v1/auth/me" &&
    request.headers["x-api-user"] === undefined &&
    request.headers["x-api-key"] === "fake-seerr-key"
  ) {
    response
      .writeHead(200)
      .end(JSON.stringify({ email: "private-owner@example.test", id: 999 }));

    return;
  }

  if (
    request.headers["x-api-key"] !== "fake-seerr-key" ||
    (actor !== 1 && actor !== 2)
  ) {
    response.writeHead(401).end('{"error":"native authentication required"}');

    return;
  }

  if (request.method === "GET") {
    const value = read(url.pathname, actor, url.searchParams);

    response
      .writeHead(value === undefined ? 404 : 200)
      .end(JSON.stringify(value));

    return;
  }

  await write(request, response, actor, url.pathname);
}

function resolveIssue(response: ServerResponse, actor: number, id: number) {
  const issue = issues.get(id);

  if (!issue || issue.createdBy.id !== actor) {
    response.writeHead(403).end('{"error":"issue not owned"}');

    return;
  }

  issue.status = 2;
  response.end(JSON.stringify(issue));
}

async function write(
  request: IncomingMessage,
  response: ServerResponse,
  actor: number,
  path: string,
) {
  if (request.method === "POST" && path === "/api/v1/request") {
    await createRequest(request, response, actor);

    return;
  }

  if (request.method === "POST" && path === "/api/v1/issue") {
    await createIssue(request, response, actor);

    return;
  }

  const canceled = /^\/api\/v1\/request\/(\d+)$/.exec(path);
  const resolved = /^\/api\/v1\/issue\/(\d+)\/resolved$/.exec(path);

  if (request.method === "DELETE" && canceled) {
    cancelRequest(response, actor, Number(canceled[1]));

    return;
  }

  if (request.method === "POST" && resolved) {
    resolveIssue(response, actor, Number(resolved[1]));

    return;
  }

  response.writeHead(405).end('{"error":"unsupported fixture operation"}');
}

createServer((request, response) => {
  void reply(request, response).catch(() => {
    response.writeHead(500).end('{"error":"fixture failure"}');
  });
}).listen(5055, "0.0.0.0");
