import { expect, test } from "@playwright/test";

const origins = ["http://127.0.0.1:4173", "http://127.0.0.1:4174"];
const headers = {
  Authorization: "Bearer fake-requester-key-1111111111111111111",
};

for (const origin of origins) {
  test(`community ratings and provider filters through ${origin}`, async ({
    request,
  }) => {
    const health = await request.post(`${origin}/v1/tools/service_status`, {
      data: { no_cache: true },
      headers,
    });

    expect(health.ok()).toBe(true);
    const status = (await health.json()) as { sources: unknown[] };

    expect(status.sources).toEqual(
      expect.arrayContaining([
        expect.objectContaining({
          kind: "plex",
          reachable: true,
          source: "plex",
          writes_supported: false,
        }),
      ]),
    );

    const library = await request.post(`${origin}/v1/tools/search_library`, {
      data: {
        available: true,
        genres: ["Comedy"],
        limit: 1,
        max_runtime_minutes: 120,
        min_rating: 7,
        min_votes: 10_000,
        no_cache: true,
        rating_provider: "imdb",
        sort: "rating",
      },
      headers,
    });

    expect(library.ok()).toBe(true);
    const result = (await library.json()) as {
      total: number;
      items: { title: string; ratings: unknown[] }[];
    };

    expect(result.total).toBeGreaterThan(1);
    expect(result.items).toHaveLength(1);
    expect(result.items[0]).toMatchObject({ title: "The Last Lantern" });
    expect(result.items[0]?.ratings).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ origin: "radarr", provider: "imdb" }),
        expect.objectContaining({
          origin: "plex",
          provider: "imdb",
          score: 8.2,
        }),
        expect.objectContaining({
          origin: "plex",
          provider: "rottenTomatoes",
          score: 9.1,
        }),
      ]),
    );
    expect(result).toMatchObject({
      rating_sources: [
        expect.objectContaining({ source: "plex", status: "ok" }),
      ],
      ratings_complete: true,
    });
    expect(JSON.stringify(result)).not.toMatch(
      /userRating|watchCount|PlexOnlineToken/,
    );

    const details = await request.post(
      `${origin}/v1/tools/media_details_batch`,
      {
        data: {
          items: [
            { media_type: "movie", tmdb_id: 99 },
            { media_type: "tv", tmdb_id: 84 },
            { media_type: "tv", tmdb_id: 85 },
          ],
          no_cache: true,
        },
        headers,
      },
    );

    expect(details.ok()).toBe(true);
    const batch = (await details.json()) as {
      succeeded: number;
      items: unknown[];
    };

    expect(batch.succeeded).toBe(3);
    expect(batch.items[2]).toMatchObject({
      ok: true,
      result: {
        rating_sources: [expect.objectContaining({ status: "unavailable" })],
        ratings: [
          expect.objectContaining({ origin: "seerr", provider: "tmdb" }),
        ],
        ratings_complete: false,
      },
    });
    const serialized = JSON.stringify(batch);

    expect(serialized).toContain('"origin":"seerr"');
    expect(serialized).toContain('"provider":"tmdb"');
    expect(serialized).toContain('"provider":"imdb"');
    expect(serialized).toContain('"provider":"rottenTomatoesAudience"');
    expect(serialized).toContain('"raw_scale":100');
    expect(serialized).toContain('"votes":24000');
    expect(serialized).not.toContain("private-fixture");
  });
}
