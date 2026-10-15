import { type NextRequest } from "next/server";

import { coreUrl } from "@/lib/core-origin";

const headers = {
  "Cache-Control": "private, no-store",
  "Content-Security-Policy":
    "default-src 'none'; frame-ancestors 'none'; base-uri 'none'",
  "Referrer-Policy": "no-referrer",
  "X-Content-Type-Options": "nosniff",
};

export async function GET(
  request: NextRequest,
  context: { params: Promise<{ token: string; format: string }> },
) {
  const { format, token } = await context.params;

  if (
    !/^[A-Za-z0-9_-]{43}$/.test(token) ||
    !["calendar.ics", "calendar.rss"].includes(format)
  ) {
    return new Response(null, { headers, status: 404 });
  }

  const query = request.nextUrl.searchParams;

  if (
    Array.from(query.keys()).some((name) => !["end", "start"].includes(name)) ||
    ["start", "end"].some(
      (name) =>
        query.getAll(name).length > 1 ||
        (query.has(name) && !/^\d{4}-\d{2}-\d{2}$/.test(query.get(name) ?? "")),
    )
  ) {
    return new Response(null, { headers, status: 400 });
  }

  try {
    const upstream = coreUrl(`/v1/${format}`);
    const controller = new AbortController();
    // Limit the wait for headers; let the body stream until the client closes.
    const timeout = setTimeout(() => controller.abort(), 20_000);

    upstream.search = query.toString();
    const response = await fetch(upstream, {
      cache: "no-store",
      headers: { "X-ArrPi-Feed-Token": token },
      redirect: "error",
      signal: AbortSignal.any([controller.signal, request.signal]),
    }).finally(() => clearTimeout(timeout));
    const resultHeaders = new Headers(headers);

    resultHeaders.set(
      "Content-Type",
      response.headers.get("Content-Type") ?? "application/octet-stream",
    );

    return new Response(response.body, {
      headers: resultHeaders,
      status: response.status,
    });
  } catch {
    return new Response(null, { headers, status: 502 });
  }
}
