import { createServer } from "node:http";

import { catalog, communityRatings, type Media } from "./media.ts";

function metadata(item: Media, detailed: boolean) {
  const scores = communityRatings(item);

  return {
    audienceRating: scores.audienceScore / 10,
    audienceRatingImage: "rottentomatoes://image.rating.upright",
    Guid: [{ id: `tmdb://${item.id}` }, { id: `imdb://tt999000${item.id}` }],
    rating: scores.criticsScore / 10,
    Rating: detailed
      ? [
          {
            image: "imdb://image.rating",
            type: "audience",
            value: item.rating,
          },
          {
            image: "rottentomatoes://image.rating.ripe",
            type: "critic",
            value: scores.criticsScore / 10,
          },
          {
            image: "rottentomatoes://image.rating.upright",
            type: "audience",
            value: scores.audienceScore / 10,
          },
        ]
      : [],
    ratingImage: "rottentomatoes://image.rating.ripe",
    ratingKey: String(item.id),
    title: item.title,
    type: item.kind === "movie" ? "movie" : "show",
    userRating: 2,
  };
}

createServer((request, response) => {
  response.setHeader("content-type", "application/json");

  if (
    request.method !== "GET" ||
    request.headers["x-plex-token"] !== "fake-upstream-api-key"
  ) {
    response.writeHead(401).end("{}");

    return;
  }

  const url = new URL(request.url ?? "/", "http://127.0.0.1");

  if (url.pathname === "/library/sections") {
    response.end(JSON.stringify({ MediaContainer: { Directory: [] } }));

    return;
  }

  if (url.pathname.startsWith("/library/metadata/")) {
    const keys = new Set(
      url.pathname.slice("/library/metadata/".length).split(","),
    );
    const items = catalog.filter((item) => keys.has(String(item.id)));

    response.end(
      JSON.stringify({
        MediaContainer: { Metadata: items.map((item) => metadata(item, true)) },
      }),
    );

    return;
  }

  if (url.pathname !== "/library/all") {
    response.writeHead(404).end("{}");

    return;
  }

  const type = url.searchParams.get("type") === "1" ? "movie" : "tv";
  const all = catalog.filter((item) => item.kind === type && item.libraryId);
  const offset = Number(url.searchParams.get("X-Plex-Container-Start") ?? 0);
  const count = Number(url.searchParams.get("X-Plex-Container-Size") ?? 100);

  response.end(
    JSON.stringify({
      MediaContainer: {
        Metadata: all
          .slice(offset, offset + count)
          .map((item) => metadata(item, false)),
        offset,
        totalSize: all.length,
      },
    }),
  );
}).listen(32_400, "0.0.0.0");
