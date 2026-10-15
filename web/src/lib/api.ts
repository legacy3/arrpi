import type { Components as components } from "@/lib/generated/contracts";

export type CacheState = components["schemas"]["CacheState"];
export type Calendar = components["schemas"]["Calendar"];
export type CalendarEvent = components["schemas"]["CalendarEvent"];
export type Endpoint = components["schemas"]["Endpoint"];
export type Source = components["schemas"]["SourceInfo"];
export type Status = components["schemas"]["Status"];

export class Api {
  #abort = new AbortController();
  #key: string | undefined;
  #unauthorized: (() => void) | undefined;

  constructor(key: string | undefined, unauthorized?: () => void) {
    if (key !== undefined && !/^[\u0021-\u007E]{32,128}$/.test(key)) {
      throw new Error(
        "Use a client key with 32–128 printable ASCII characters",
      );
    }

    this.#key = key;
    this.#unauthorized = unauthorized;
  }

  close() {
    this.#abort.abort();
    this.#key = "";
  }

  read<T>(path: string, fresh = false, signal?: AbortSignal) {
    return this.#request<T>(path, fresh, signal);
  }

  write<T>(path: string, body: unknown) {
    return this.#request<T>(path, false, undefined, body);
  }

  #checkRedirect(response: Response) {
    if (
      response.type === "opaqueredirect" ||
      (response.status >= 300 && response.status < 400)
    ) {
      this.close();
      this.#unauthorized?.();
      throw new HttpError("authentication required", 401, null);
    }
  }

  #headers(fresh: boolean, hasBody: boolean) {
    return {
      ...(this.#key ? { Authorization: `Bearer ${this.#key}` } : {}),
      ...(hasBody ? { "Content-Type": "application/json" } : {}),
      ...(fresh ? { "Cache-Control": "no-cache" } : {}),
    };
  }

  async #request<T>(
    path: string,
    fresh: boolean,
    signal?: AbortSignal,
    body?: unknown,
  ) {
    const response = await fetch(path, {
      body: body === undefined ? undefined : JSON.stringify(body),
      cache: "no-store",
      headers: this.#headers(fresh, body !== undefined),
      method: body === undefined ? "GET" : "PUT",
      redirect: "manual",
      signal: signal
        ? AbortSignal.any([this.#abort.signal, signal])
        : this.#abort.signal,
    });

    this.#checkRedirect(response);
    let data: unknown = null;
    let parsed = false;

    if (
      response.headers
        .get("content-type")
        ?.toLowerCase()
        .includes("application/json")
    ) {
      try {
        data = await response.json();
        parsed = true;
      } catch {
        data = null;
      }
    }

    if (!response.ok) {
      const message =
        data &&
        typeof data === "object" &&
        "error" in data &&
        typeof data.error === "string"
          ? data.error
          : `Request failed (${response.status})`;

      if (response.status === 401) {
        this.close();
        this.#unauthorized?.();
      }

      throw new HttpError(message, response.status, data);
    }

    if (!parsed) {
      throw new Error(`Expected JSON (${response.status})`);
    }

    return {
      age: response.headers.get("age"),
      cache: response.headers.get("x-arrpi-cache"),
      data: data as T,
    };
  }
}

export class HttpError extends Error {
  data: unknown;
  status: number;

  constructor(message: string, status: number, data: unknown) {
    super(message);
    this.status = status;
    this.data = data;
  }
}

export const kindLabels: Record<CalendarEvent["kind"], string> = {
  cinema: "Cinema",
  digital: "Digital",
  episode: "Episode",
  physical: "Physical",
};

/** Month defaults and date windows share the calendar's explicit UTC convention. */
export function currentMonth(now = new Date()) {
  return `${String(now.getUTCFullYear()).padStart(4, "0")}-${String(now.getUTCMonth() + 1).padStart(2, "0")}`;
}

export function dateRange(month: string) {
  if (!isMonth(month)) {
    throw new Error("Choose a month");
  }

  const [year, number] = month.split("-").map(Number);

  if (!year || !number || number < 1 || number > 12) {
    throw new Error("Choose a month");
  }

  const date = new Date(0);

  date.setUTCFullYear(year, number, 0);
  const days = date.getUTCDate();

  return { end: `${month}-${days}`, start: `${month}-01` };
}

export function displayDate(event: CalendarEvent) {
  if (event.kind === "episode") {
    const timestamp = new Date(event.date);

    if (!event.date.includes("T") || !Number.isFinite(timestamp.getTime())) {
      return "Unknown date";
    }

    return new Intl.DateTimeFormat(undefined, {
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
      month: "short",
      timeZone: "UTC",
    }).format(timestamp);
  }

  // A movie release date is a calendar day, not a UTC timestamp.
  const day = new Date(`${event.date.slice(0, 10)}T12:00:00Z`);

  if (!Number.isFinite(day.getTime())) {
    return "Unknown date";
  }

  return new Intl.DateTimeFormat(undefined, {
    day: "numeric",
    month: "short",
    timeZone: "UTC",
  }).format(day);
}

export function isMonth(month: string) {
  return /^\d{4}-(0[1-9]|1[0-2])$/.test(month) && Number(month.slice(0, 4)) > 0;
}
