import { Film, Tv } from "lucide-react";

import type { CalendarEvent } from "@/lib/api";

import { Badge } from "@/components/ui/badge";
import { displayDate, kindLabels } from "@/lib/api";
import { cn } from "@/lib/utils";

export function CalendarEvents({
  events,
}: Readonly<{ events: CalendarEvent[] }>) {
  return (
    <ul className="event-list divide-y">
      {events.map((event) => (
        <EventRow
          key={`${event.source}-${event.id}-${event.kind}-${event.date}`}
          event={event}
        />
      ))}
    </ul>
  );
}

function EventRow({ event }: Readonly<{ event: CalendarEvent }>) {
  const episode = event.kind === "episode";
  const Component = episode ? Tv : Film;
  let subtitle = event.source;

  if (event.series) {
    subtitle = event.title;

    if (event.season !== undefined && event.episode !== undefined) {
      subtitle = `S${String(event.season).padStart(2, "0")} E${String(event.episode).padStart(2, "0")} · ${event.title}`;
    }
  }

  return (
    <li className="grid grid-cols-[36px_minmax(0,1fr)] items-center gap-x-3 gap-y-3 px-5 py-4 sm:grid-cols-[130px_36px_minmax(0,1fr)_auto] sm:gap-x-4">
      <time
        dateTime={event.date}
        className="col-span-2 col-start-1 row-start-1 text-xs tabular-nums text-muted-foreground sm:col-span-1"
      >
        {displayDate(event)}
      </time>
      <span
        className={cn(
          "col-start-1 row-start-2 flex size-9 items-center justify-center rounded-lg border sm:col-start-2 sm:row-start-1",
          episode
            ? "border-emerald-400/15 bg-emerald-400/10 text-emerald-300"
            : "border-primary/20 bg-primary/10 text-primary",
        )}
      >
        <Component className="size-4" />
      </span>
      <div className="col-start-2 row-start-2 min-w-0 space-y-1 sm:col-start-3 sm:row-start-1">
        <strong className="block text-sm font-medium wrap-anywhere">
          {event.series || event.title || "Untitled"}
        </strong>
        <span className="block text-xs leading-relaxed wrap-anywhere text-muted-foreground">
          {subtitle}
        </span>
      </div>
      <Badge
        variant="outline"
        className={cn(
          "col-start-2 row-start-1 justify-self-end text-xs sm:col-start-4",
          episode
            ? "border-emerald-400/20 text-emerald-300"
            : "border-primary/20 text-primary",
        )}
      >
        {kindLabels[event.kind]}
      </Badge>
    </li>
  );
}
