"use client";

import { useInfiniteQuery, useQuery } from "@tanstack/react-query";
import { useDebounce, useResetState } from "ahooks";

import type { Api } from "@/lib/api";
import type { Command } from "@/lib/explorer";

import { Notice } from "@/components/notice";
import { SearchField } from "@/components/search-field";
import { SearchableChoice } from "@/components/searchable-choice";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

export const guidedCommands = new Set([
  "list_movies",
  "list_series",
  "media_command_status",
  "media_details",
  "refresh_media",
  "request_movie",
  "request_series",
  "search_library",
  "search_media",
  "search_releases",
  "series_episodes",
]);
type Media = {
  id?: number;
  tmdb_id?: number;
  source?: string;
  title: string;
  year?: number;
  media_type?: string;
};
type SearchPage = {
  items: Media[];
  next_offset?: number | null;
  page?: number;
  total_pages?: number;
};

type Values = Record<string, unknown>;

export function CommandArguments({
  api,
  command,
  lockedSource,
  onChange,
  series,
  value,
}: Readonly<{
  api: Api;
  command: Command;
  lockedSource?: string;
  series: boolean;
  value: Values;
  onChange: (value: Values) => void;
}>) {
  const native = [
    "refresh_media",
    "search_releases",
    "series_episodes",
  ].includes(command.name);
  const request = ["media_details", "request_movie", "request_series"].includes(
    command.name,
  );
  const search = [
    "list_movies",
    "list_series",
    "search_library",
    "search_media",
  ].includes(command.name);
  const requestKind = new Map([
    ["request_movie", "movie"],
    ["request_series", "tv"],
  ]).get(command.name);
  const change = (key: string, entry: unknown) =>
    onChange({ ...value, [key]: entry });

  return (
    <div className="grid gap-4">
      {!lockedSource && (native || command.name === "media_command_status") && (
        <div className="grid gap-2">
          <Label htmlFor="argument-source">Source</Label>
          <SearchableChoice
            id="argument-source"
            label="Source"
            value={stringValue(value.source)}
            options={(command.sources ?? []).map((source) => ({
              label: source,
              value: source,
            }))}
            onChange={(source) => {
              const next: Values = { ...value, source };

              delete next.item_id;
              delete next.episode_ids;
              delete next.season;
              delete next.missing_only;
              delete next.aired_only;
              onChange(next);
            }}
            className="w-full sm:max-w-sm"
          />
        </div>
      )}
      {(native || request) && (
        <MediaPicker
          api={api}
          onSearch={() => onChange(value)}
          native={native}
          kind={requestKind}
          source={typeof value.source === "string" ? value.source : undefined}
          id={value[native ? "item_id" : "tmdb_id"]}
          onSelect={(item) =>
            onChange({
              ...value,
              ...(native
                ? { item_id: item.id, source: item.source }
                : {
                    tmdb_id: item.tmdb_id,
                    ...(command.name === "media_details"
                      ? { media_type: item.media_type }
                      : {}),
                  }),
            })
          }
        />
      )}
      {search && (
        <SearchField
          label="Search titles"
          value={stringValue(value.query)}
          onChange={(query) => change("query", query)}
        />
      )}
      {command.name === "request_series" && (
        <SeasonsInput
          value={value.seasons}
          onChange={(seasons) => change("seasons", seasons)}
        />
      )}
      {series &&
        (command.name === "search_releases" ||
          command.name === "series_episodes") && (
          <div className="grid gap-2 sm:max-w-sm">
            <Label htmlFor="argument-season">Season (optional)</Label>
            <Input
              id="argument-season"
              type="number"
              min={0}
              max={2_147_483_647}
              value={typeof value.season === "number" ? value.season : ""}
              onChange={(event) => {
                const next = { ...value };

                if (event.target.value) {
                  next.season = event.target.valueAsNumber;
                } else {
                  delete next.season;
                }

                onChange(next);
              }}
            />
          </div>
        )}
      {(command.name === "search_releases" ||
        command.name === "series_episodes") && (
        <label className="flex items-center gap-2 text-sm">
          <Checkbox
            checked={
              value.missing_only === undefined
                ? command.name === "search_releases"
                : value.missing_only === true
            }
            onCheckedChange={(checked) =>
              change("missing_only", checked === true)
            }
          />
          {series ? "Missing episodes only" : "Missing movie only"}
        </label>
      )}
      {command.name === "series_episodes" && (
        <label className="flex items-center gap-2 text-sm">
          <Checkbox
            checked={value.aired_only === true}
            onCheckedChange={(checked) =>
              change("aired_only", checked === true)
            }
          />
          Aired episodes only
        </label>
      )}
      {command.name === "media_command_status" && (
        <div className="grid gap-2">
          <Label htmlFor="argument-command-id">Command ID</Label>
          <Input
            id="argument-command-id"
            type="number"
            required
            min={1}
            max={2_147_483_647}
            value={Number(value.command_id) || ""}
            onChange={(event) =>
              change("command_id", event.target.valueAsNumber)
            }
          />
        </div>
      )}
      {command.name === "media_details" && (
        <div className="grid gap-2">
          <Label htmlFor="argument-media-type">Media type</Label>
          <SearchableChoice
            id="argument-media-type"
            label="Media type"
            value={stringValue(value.media_type) || "movie"}
            onChange={(kind) => change("media_type", kind)}
            options={[
              { label: "Movie", value: "movie" },
              { label: "TV show", value: "tv" },
            ]}
          />
        </div>
      )}
      {(command.name === "request_movie" ||
        command.name === "request_series") && (
        <ProfilePicker
          api={api}
          kind={command.name === "request_movie" ? "movie" : "tv"}
          value={typeof value.profile === "string" ? value.profile : ""}
          onChange={(profile) => {
            const next = { ...value };

            if (profile) {
              next.profile = profile;
            } else {
              delete next.profile;
            }

            onChange(next);
          }}
        />
      )}
    </div>
  );
}

function MediaPicker({
  api,
  id,
  kind,
  native,
  onSearch,
  onSelect,
  source,
}: Readonly<{
  api: Api;
  native: boolean;
  kind?: string;
  source?: string;
  id: unknown;
  onSelect: (item: Media) => void;
  onSearch: () => void;
}>) {
  const [search, setSearch] = useResetState("");
  const [title, setTitle] = useResetState("");
  const query = useDebounce(search, { wait: 200 });
  const results = useInfiniteQuery({
    enabled: query.trim().length >= 2,
    initialPageParam: native ? 0 : 1,
    queryFn: async ({ pageParam }): Promise<SearchPage> =>
      (
        await api.command<SearchPage>(
          native ? "search_library" : "search_media",
          { query, ...(native ? { offset: pageParam } : { page: pageParam }) },
        )
      ).data,
    queryKey: ["media-lookup", native, kind, source, query],
    retry: false,
    staleTime: 30_000,
    // The result cursor follows queryFn for TanStack's type inference.
    getNextPageParam: (last: SearchPage) => {
      if (native) {
        return last.next_offset ?? undefined;
      }

      return last.page &&
        last.total_pages &&
        last.page < Math.min(100, last.total_pages)
        ? last.page + 1
        : undefined;
    },
  });
  const items = results.data?.pages.flatMap((page) => page.items) ?? [];
  const unique = [
    ...new Map(
      items.map((item) => [
        `${item.source ?? item.media_type}:${item.id ?? item.tmdb_id}`,
        item,
      ]),
    ).values(),
  ];
  const matches = unique.filter(
    (item) =>
      (!native || !source || item.source === source) &&
      (!kind || item.media_type === kind),
  );

  return (
    <div className="grid gap-2">
      <SearchField
        label={native ? "Find installed media" : "Find a movie or TV show"}
        value={search}
        onChange={(value) => {
          setSearch(value);
          onSearch();
        }}
      />
      {id != null && (
        <p className="text-sm font-medium">
          Selected:{" "}
          {title || `Media #${typeof id === "number" ? String(id) : "Unknown"}`}
        </p>
      )}
      {results.error && <Notice>{results.error.message}</Notice>}
      {results.isFetching && (
        <p role="status" className="text-sm text-muted-foreground">
          Searching...
        </p>
      )}
      {query.length >= 2 && results.data && (
        <ul className="max-h-56 overflow-y-auto rounded-lg border divide-y">
          {matches.map((item) => (
            <li
              key={`${item.source ?? item.media_type}-${item.id ?? item.tmdb_id}`}
            >
              <Button
                type="button"
                variant="ghost"
                className="h-auto w-full justify-start whitespace-normal rounded-none px-3 py-2 text-left"
                onClick={() => {
                  setTitle(item.title);
                  onSelect(item);
                  setSearch("");
                }}
              >
                <span>
                  {item.title}
                  {item.year ? ` (${item.year})` : ""}
                </span>
              </Button>
            </li>
          ))}
          {matches.length === 0 && (
            <li className="px-3 py-3 text-sm text-muted-foreground">
              No matching media in these results. Refine the title.
            </li>
          )}
        </ul>
      )}
      {results.hasNextPage && (
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={results.isFetchingNextPage}
          onClick={() => void results.fetchNextPage()}
        >
          Load more results
        </Button>
      )}
    </div>
  );
}

function ProfilePicker({
  api,
  kind,
  onChange,
  value,
}: Readonly<{
  api: Api;
  kind: string;
  value: string;
  onChange: (value: string) => void;
}>) {
  const query = useQuery({
    queryFn: async () =>
      (
        await api.command<{ items: { profile: string | null }[] }>(
          "request_profiles",
          { media_type: kind, no_cache: true },
        )
      ).data,
    queryKey: ["request-profiles", kind],
    retry: false,
  });
  const profiles = query.data?.items.map((item) => ({
    label: item.profile ?? "Default profile",
    value: item.profile == null ? "default" : `profile:${item.profile}`,
  })) ?? [{ label: "Default profile", value: "default" }];

  return (
    <div className="grid gap-2">
      <Label>Request profile</Label>
      <SearchableChoice
        label="Request profile"
        value={value ? `profile:${value}` : "default"}
        onChange={(selected) =>
          onChange(
            selected === "default" ? "" : selected.slice("profile:".length),
          )
        }
        options={profiles}
      />
      {query.error && (
        <p className="text-xs text-muted-foreground">
          Default profile will be used. {query.error.message}
        </p>
      )}
    </div>
  );
}

function SeasonsInput({
  onChange,
  value,
}: Readonly<{ value: unknown; onChange: (seasons: number[]) => void }>) {
  const [draft, setDraft] = useResetState(
    Array.isArray(value)
      ? value
          .filter((entry): entry is number => typeof entry === "number")
          .join(", ")
      : "",
  );

  return (
    <div className="grid gap-2">
      <Label htmlFor="argument-seasons">Seasons</Label>
      <Input
        id="argument-seasons"
        placeholder="1, 2, 3"
        required
        pattern={String.raw`\s*[0-9]+(?:\s*,\s*[0-9]+)*\s*`}
        value={draft}
        onChange={(event) => {
          const input = event.target.value;

          setDraft(input);
          onChange(
            input
              .split(",")
              .map((part) => part.trim())
              .filter(Boolean)
              .map(Number),
          );
        }}
      />
      <p className="text-xs text-muted-foreground">
        Season numbers separated by commas.
      </p>
    </div>
  );
}

function stringValue(value: unknown) {
  return typeof value === "string" ? value : "";
}
