"use client";

import { useResetState } from "ahooks";
import { CircleCheck, Clock3, ExternalLink, Film, Tv } from "lucide-react";

import type { CalendarEvent } from "@/lib/api";

import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { displayDate, kindLabels } from "@/lib/api";
import { cn } from "@/lib/utils";

export function CalendarEvents({
  events,
}: Readonly<{ events: CalendarEvent[] }>) {
  const [selected, setSelected, close] = useResetState<CalendarEvent | null>(
    null,
  );

  return (
    <>
      <ul className="event-list divide-y">
        {events.map((event) => (
          <EventRow
            key={`${event.source}-${event.id}-${event.kind}-${event.date}`}
            event={event}
            onSelect={() => setSelected(event)}
          />
        ))}
      </ul>
      <Dialog
        open={Boolean(selected)}
        onOpenChange={(open) => {
          if (!open) {
            close();
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{selected?.series || selected?.title}</DialogTitle>
            <DialogDescription>
              {selected ? displayDate(selected) : ""}
            </DialogDescription>
          </DialogHeader>
          {selected && (
            <>
              <p className="text-sm">
                {selected.series
                  ? `${episodeLabel(selected)} ${selected.title}`
                  : kindLabels[selected.kind]}
              </p>
              <p className="flex items-center gap-2 text-sm text-muted-foreground">
                {selected.has_file ? (
                  <CircleCheck className="size-4 text-emerald-300" />
                ) : (
                  <Clock3 className="size-4" />
                )}
                {selected.has_file
                  ? "Available in the library"
                  : "Not available in the library"}
              </p>
              <div className="flex flex-wrap gap-4 text-sm text-primary">
                {selected.imdb_id && /^tt\d+$/.test(selected.imdb_id) && (
                  <a
                    href={`https://www.imdb.com/title/${selected.imdb_id}`}
                    target="_blank"
                    rel="noreferrer"
                    className="inline-flex items-center gap-1"
                  >
                    IMDb
                    <ExternalLink className="size-3.5" />
                  </a>
                )}
                {Boolean(selected.tmdb_id) && (
                  <a
                    href={`https://www.themoviedb.org/${selected.kind === "episode" ? "tv" : "movie"}/${selected.tmdb_id}`}
                    target="_blank"
                    rel="noreferrer"
                    className="inline-flex items-center gap-1"
                  >
                    TMDB
                    <ExternalLink className="size-3.5" />
                  </a>
                )}
              </div>
              <p className="text-xs text-muted-foreground">
                Source: {selected.source}
              </p>
            </>
          )}
        </DialogContent>
      </Dialog>
    </>
  );
}

function episodeLabel(event: CalendarEvent) {
  return event.season == null || event.episode == null
    ? ""
    : `S${String(event.season).padStart(2, "0")} E${String(event.episode).padStart(2, "0")}`;
}

function EventRow({
  event,
  onSelect,
}: Readonly<{ event: CalendarEvent; onSelect: () => void }>) {
  const episode = event.kind === "episode";
  const Icon = episode ? Tv : Film;

  return (
    <li className="grid grid-cols-[36px_minmax(0,1fr)] items-center gap-x-3 gap-y-2 px-5 py-3 sm:grid-cols-[150px_36px_minmax(0,1fr)_auto] sm:gap-x-4">
      <time
        dateTime={event.date}
        className="col-span-2 col-start-1 row-start-1 text-xs tabular-nums text-muted-foreground sm:col-span-1"
      >
        {displayDate(event)}
      </time>
      <span
        className={cn(
          "col-start-1 row-start-2 flex size-9 items-center justify-center rounded-lg border text-primary sm:col-start-2 sm:row-start-1",
          episode && "text-emerald-300",
        )}
      >
        <Icon className="size-4" />
      </span>
      <div className="col-start-2 row-start-2 min-w-0 space-y-1 sm:col-start-3 sm:row-start-1">
        <button
          type="button"
          onClick={onSelect}
          className="block text-left text-sm font-medium wrap-anywhere hover:text-primary hover:underline"
        >
          {event.series || event.title || "Untitled"}
        </button>
        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
          {episodeLabel(event) && <span>{episodeLabel(event)}</span>}
          <span className="wrap-anywhere">
            {event.series ? event.title : event.source}
          </span>
          <span className="inline-flex items-center gap-1">
            {event.has_file ? (
              <CircleCheck className="size-3.5 text-emerald-300" />
            ) : (
              <Clock3 className="size-3.5" />
            )}
            {event.has_file ? "Available" : "Not available"}
          </span>
        </div>
      </div>
      <Badge
        variant="outline"
        className="col-start-2 row-start-1 justify-self-end text-xs sm:col-start-4"
      >
        {kindLabels[event.kind]}
      </Badge>
    </li>
  );
}
