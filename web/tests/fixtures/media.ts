export type Kind = "movie" | "tv";
export type Media = {
  availableSeasons?: number[];
  genres: string[];
  id: number;
  kind: Kind;
  libraryId?: number;
  overview: string;
  rating: number;
  runtime: number;
  seasons?: number[];
  title: string;
  year: number;
};

// Deliberately fictional, shared by the native libraries and request catalog.
export const catalog: Media[] = [
  {
    genres: ["Comedy"],
    id: 99,
    kind: "movie",
    libraryId: 2,
    overview:
      "A lighthouse keeper and a travelling baker try to save their town’s winter festival.",
    rating: 8.2,
    runtime: 110,
    title: "The Last Lantern",
    year: 2025,
  },
  {
    genres: ["Comedy", "Adventure"],
    id: 100,
    kind: "movie",
    libraryId: 3,
    overview:
      "A retired pirate opens a tea shop and accidentally enters the village baking contest.",
    rating: 7.8,
    runtime: 96,
    title: "Captain Chamomile",
    year: 2024,
  },
  {
    genres: ["Comedy", "Animation"],
    id: 101,
    kind: "movie",
    libraryId: 4,
    overview:
      "Three delivery cats borrow a hot-air balloon to return a birthday parcel before sunrise.",
    rating: 7.4,
    runtime: 88,
    title: "Parcel Cats",
    year: 2023,
  },
  {
    genres: ["Drama", "Science Fiction"],
    id: 102,
    kind: "movie",
    libraryId: 5,
    overview:
      "An observatory caretaker hears a familiar song in a signal from a distant moon.",
    rating: 8.5,
    runtime: 118,
    title: "The Quiet Orbit",
    year: 2022,
  },
  {
    genres: ["Comedy", "Adventure"],
    id: 42,
    kind: "movie",
    overview:
      "Two rival bakers chase a runaway cake across a sleepy seaside town.",
    rating: 8.1,
    runtime: 98,
    title: "Moonberry Express",
    year: 2026,
  },
  {
    availableSeasons: [1],
    genres: ["Drama", "Science Fiction"],
    id: 84,
    kind: "tv",
    libraryId: 1,
    overview:
      "The night shift at a remote space station discovers a mysterious teapot.",
    rating: 8.3,
    runtime: 45,
    seasons: [10, 8, 6],
    title: "Moonberry Station",
    year: 2024,
  },
  {
    availableSeasons: [1, 2],
    genres: ["Comedy", "Mystery"],
    id: 85,
    kind: "tv",
    libraryId: 2,
    overview:
      "Two librarians solve village mysteries using overdue book slips and very strong tea.",
    rating: 7.9,
    runtime: 28,
    seasons: [6, 6],
    title: "The Midnight Book Club",
    year: 2023,
  },
  // Dedicated finite request candidates keep parallel browser runs independent.
  ...[
    "Pebble Post Office",
    "The Velvet Ferry",
    "Cloudberry Picnic",
    "The Paper Planet",
    "Ruby Post Office",
    "The Willow Ferry",
    "Apricot Picnic",
    "The Velvet Planet",
  ].map((title, index): Media => ({
    genres: ["Adventure"],
    id: 142 + index,
    kind: "movie",
    overview:
      "An unlikely team sets out to deliver a lost letter before the summer ends.",
    rating: 7.1,
    runtime: 104,
    title,
    year: 2026,
  })),
  ...[
    "Harbour Signals",
    "The Teapot Archive",
    "Mosslight Valley",
    "The Clockwork Garden",
    "River Signals",
    "The Teacup Archive",
    "Fernlight Valley",
    "The Mechanical Garden",
  ].map((title, index): Media => ({
    availableSeasons: [1],
    genres: ["Mystery"],
    id: 184 + index,
    kind: "tv",
    overview:
      "A small-town radio crew follows a trail of messages left by a vanished gardener.",
    rating: 7.6,
    runtime: 42,
    seasons: [6, 8],
    title,
    year: 2025,
  })),
];

export function communityRatings(item: Media) {
  return {
    audienceScore: item.id === 99 ? 87 : 80 + (item.id % 15),
    criticsScore: item.id === 99 ? 91 : 78 + (item.id % 12),
  };
}

export function findMedia(kind: Kind, id: number) {
  return catalog.find((item) => item.kind === kind && item.id === id);
}

export function nativeMedia(item: Media) {
  const common = {
    genres: item.genres,
    id: item.libraryId,
    imdbId: `tt999000${item.id}`,
    monitored: true,
    overview: item.overview,
    path: `/private/${item.kind === "movie" ? "movies" : "tv"}`,
    qualityProfileId: 1,
    ratings: { imdb: { value: item.rating, votes: 24_000 } },
    runtime: item.runtime,
    title: item.title,
    tmdbId: item.id,
    year: item.year,
  };

  return item.kind === "movie"
    ? { ...common, hasFile: true }
    : {
        ...common,
        statistics: {
          episodeCount: item.seasons?.reduce((sum, count) => sum + count, 0),
          episodeFileCount: item.availableSeasons?.reduce(
            (sum, season) => sum + (item.seasons?.[season - 1] ?? 0),
            0,
          ),
        },
        tvdbId: 999_000 + item.id,
      };
}
