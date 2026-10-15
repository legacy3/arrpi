import { type APIRequestContext, expect, test } from "@playwright/test";

const accountKey = "fake-requester-key-0000000000000000000";
const otherKey = "fake-requester-key-1111111111111111111";
const origins = process.env.ARRPI_TEST_ORIGINS?.split(",") ?? [
  "http://127.0.0.1:4173",
  "http://127.0.0.1:4174",
];

async function approvedRequest(
  request: APIRequestContext,
  origin: string,
  kind: "movie" | "tv",
  id: number,
) {
  const created = await tool(
    request,
    origin,
    otherKey,
    kind === "movie" ? "request_movie" : "request_series",
    {
      tmdb_id: id,
      ...(kind === "tv" ? { seasons: [2] } : {}),
    },
  );

  expect(created.response.ok()).toBe(true);
  // Repeated runs retain approved native requests; they must stay idempotent.
  expect(["approved", "already_requested"]).toContain(created.data.status);
  const mine = await tool(request, origin, otherKey, "my_requests", {
    limit: 100,
    no_cache: true,
  });
  const rows = mine.data.items as Record<string, unknown>[];
  const receipt = rows.find(
    (item) => item.tmdb_id === id && item.media_type === kind,
  );

  expect(mine.response.ok()).toBe(true);
  expect(receipt).toMatchObject({
    is_4k: false,
    seasons: kind === "tv" ? [2] : [],
    status: "approved",
  });
  const status = await tool(request, origin, otherKey, "request_status", {
    request_id: receipt?.request_id,
  });

  expect(status.response.ok()).toBe(true);
  expect(status.data).toMatchObject({ status: "approved", tmdb_id: id });
  const details = await tool(request, origin, otherKey, "media_details", {
    media_type: kind,
    no_cache: true,
    tmdb_id: id,
  });

  expect(details.response.ok()).toBe(true);
  expect(details.data.available).toBe(false);

  if (kind === "movie") {
    expect(details.data).toMatchObject({
      requested: true,
      status_name: "processing",
    });
  } else {
    expect(details.data).toMatchObject({
      season_availability: [
        { available: true, requested: false, season: 1 },
        {
          available: false,
          requested: true,
          season: 2,
          status_name: "processing",
        },
      ],
    });
  }

  const denied = await tool(request, origin, otherKey, "cancel_request", {
    request_id: receipt?.request_id,
  });

  expect(denied.response.status()).toBe(409);
  expect(denied.data).toMatchObject({ code: "request_not_pending" });
}

async function tool(
  request: APIRequestContext,
  origin: string,
  key: string,
  name: string,
  args: Record<string, unknown>,
) {
  const response = await request.post(`${origin}/v1/tools/${name}`, {
    data: args,
    headers: { Authorization: `Bearer ${key}` },
  });

  return { data: (await response.json()) as Record<string, unknown>, response };
}

test("built core and UI use connected Seerr and preserve bound requester identity", async ({
  request,
}, testInfo) => {
  for (const [originIndex, origin] of origins.entries()) {
    const headers = { Authorization: `Bearer ${accountKey}` };
    const list = await request.get(`${origin}/v1/tools`, { headers });

    expect(list.ok()).toBe(true);
    expect(await list.text()).toContain('"name":"request_movie"');
    const search = await tool(request, origin, accountKey, "search_media", {
      query: "Moonberry Express",
    });

    expect(search.response.ok()).toBe(true);
    expect(search.data).toMatchObject({ items: [{ tmdb_id: 42 }] });
    expect(JSON.stringify(search.data)).not.toContain("private-fixture");
    const missing = await tool(request, origin, accountKey, "search_media", {
      no_cache: true,
      query: "No fictional title matches this phrase",
    });

    expect(missing.response.ok()).toBe(true);
    expect(missing.data).toMatchObject({ items: [] });

    const tmdbId =
      142 + originIndex + (testInfo.project.name === "mobile" ? 2 : 0);
    const created = await tool(request, origin, accountKey, "request_movie", {
      tmdb_id: tmdbId,
    });

    expect(created.response.ok()).toBe(true);
    expect(created.data).toMatchObject({
      is_4k: false,
      status: "pending",
      tmdb_id: tmdbId,
    });
    const owned = await tool(request, origin, accountKey, "request_status", {
      request_id: created.data.request_id,
    });
    const denied = await tool(request, origin, otherKey, "request_status", {
      request_id: created.data.request_id,
    });

    expect(owned.response.ok()).toBe(true);
    expect(denied.response.status()).toBe(403);
    expect(denied.data).toMatchObject({ code: "permission_denied" });
    expect(JSON.stringify(owned.data)).not.toContain("requestedBy");
    const cancelDenied = await tool(
      request,
      origin,
      otherKey,
      "cancel_request",
      {
        request_id: created.data.request_id,
      },
    );

    expect(cancelDenied.response.status()).toBe(403);

    const details = await tool(request, origin, accountKey, "media_details", {
      media_type: "movie",
      no_cache: true,
      tmdb_id: tmdbId,
    });

    expect(details.response.ok()).toBe(true);
    expect(details.data).toMatchObject({
      available: false,
      requested: true,
      status_name: "pending",
    });
    const canceled = await tool(request, origin, accountKey, "cancel_request", {
      request_id: created.data.request_id,
    });

    expect(canceled.response.ok()).toBe(true);
    expect(canceled.data).toMatchObject({ status: "cancelled" });
    const after = await tool(request, origin, accountKey, "media_details", {
      media_type: "movie",
      no_cache: true,
      tmdb_id: tmdbId,
    });

    expect(after.data).toMatchObject({
      requested: false,
      status_name: "unknown",
    });

    const mcp = await request.post(`${origin}/v1/mcp`, {
      data: {
        id: 1,
        jsonrpc: "2.0",
        method: "tools/call",
        params: { arguments: { no_cache: true }, name: "my_quota" },
      },
      headers: {
        ...headers,
        Accept: "application/json, text/event-stream",
        "MCP-Protocol-Version": "2025-06-18",
      },
    });

    expect(mcp.ok()).toBe(true);
    expect(await mcp.json()).toMatchObject({
      result: {
        isError: false,
        structuredContent: { movie: { unlimited: true } },
      },
    });
  }
});

test("fictional season requests and owned issues stay consistent across tools", async ({
  request,
}, testInfo) => {
  for (const [originIndex, origin] of origins.entries()) {
    const tmdbId =
      184 + originIndex + (testInfo.project.name === "mobile" ? 2 : 0);
    const created = await tool(request, origin, accountKey, "request_series", {
      seasons: [1, 2],
      tmdb_id: tmdbId,
    });

    expect(created.response.ok()).toBe(true);
    expect(created.data).toMatchObject({
      accepted_seasons: [2],
      skipped_seasons: [1],
      status: "pending",
    });
    const details = await tool(request, origin, accountKey, "media_details", {
      media_type: "tv",
      no_cache: true,
      tmdb_id: tmdbId,
    });

    expect(details.response.ok()).toBe(true);
    expect(details.data).toMatchObject({
      partially_available: true,
      season_availability: [
        { available: true, requested: false, season: 1 },
        { available: false, requested: true, season: 2 },
      ],
    });
    const canceled = await tool(request, origin, accountKey, "cancel_request", {
      request_id: created.data.request_id,
    });

    expect(canceled.response.ok()).toBe(true);
    const reported = await tool(request, origin, accountKey, "report_issue", {
      category: "audio",
      media_type: "movie",
      message: "The left audio channel is missing in this fictional test.",
      tmdb_id: 99,
    });

    expect(reported.response.ok()).toBe(true);
    expect(reported.data).toMatchObject({
      category: 2,
      status_name: "open",
      tmdb_id: 99,
    });
    const issueId = reported.data.issue_id;
    const denied = await tool(request, origin, otherKey, "close_issue", {
      issue_id: issueId,
    });

    expect(denied.response.status()).toBe(403);
    const closed = await tool(request, origin, accountKey, "close_issue", {
      issue_id: issueId,
    });

    expect(closed.response.ok()).toBe(true);
    expect(closed.data).toMatchObject({
      issue_id: issueId,
      status: 2,
      status_name: "resolved",
    });
    let offset = 0;
    let found = false;

    while (!found) {
      const mine = await tool(request, origin, accountKey, "list_issues", {
        no_cache: true,
        offset,
      });

      expect(mine.response.ok()).toBe(true);
      const items = mine.data.items as {
        issue_id: number;
        status_name: string;
      }[];

      found = items.some(
        (issue) =>
          issue.issue_id === issueId && issue.status_name === "resolved",
      );
      const next = mine.data.next_offset;

      if (found || typeof next !== "number") {
        break;
      }

      expect(next).toBeGreaterThan(offset);
      offset = next;
    }

    expect(found).toBe(true);
    const links = await tool(
      request,
      origin,
      accountKey,
      "media_details_batch",
      {
        items: [
          { media_type: "movie", tmdb_id: 99 },
          { media_type: "movie", tmdb_id: 42 },
          { media_type: "tv", tmdb_id: 84 },
        ],
        no_cache: true,
      },
    );

    expect(links.response.ok()).toBe(true);
    const serialized = JSON.stringify(links.data);

    expect(serialized).toContain("https://request.example.test/movie/99");
    expect(serialized).toContain("https://watch.example.test/");
    expect(serialized).toContain("https://www.imdb.com/title/tt99900099");
    expect(serialized).not.toContain("private-fixture");
    expect(serialized).not.toContain("requestedBy");
  }
});

test("demo account reads upcoming releases and auto-approves without claiming availability", async ({
  request,
}, testInfo) => {
  for (const [originIndex, origin] of origins.entries()) {
    const upcoming = await tool(
      request,
      origin,
      otherKey,
      "upcoming_releases",
      {
        end: "2026-10-22",
        no_cache: true,
        start: "2026-10-15",
      },
    );

    expect(upcoming.response.ok()).toBe(true);
    expect(upcoming.data).toMatchObject({
      events: [
        {
          date: "2026-10-15T21:00:00Z",
          episode: 7,
          has_file: false,
          kind: "episode",
          season: 2,
          series: "Moonberry Station",
        },
        { date: "2026-10-16", kind: "digital", title: "The Last Lantern" },
      ],
      sources: [
        { source: "radarr", status: "ok" },
        { source: "sonarr", status: "ok" },
      ],
    });
    const slot = originIndex + (testInfo.project.name === "mobile" ? 2 : 0);
    const movieId = 146 + slot;
    const seriesId = 188 + slot;

    for (const [kind, id] of [
      ["movie", movieId],
      ["tv", seriesId],
    ] as const) {
      await approvedRequest(request, origin, kind, id);
    }
  }
});
