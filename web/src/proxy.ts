import { type NextRequest, NextResponse } from "next/server";

import { coreUrl } from "@/lib/core-origin";

export function proxy(request: NextRequest) {
  const path = request.nextUrl.pathname;

  if (
    path === "/v1" ||
    path.startsWith("/v1/") ||
    [
      "/.well-known/oauth-protected-resource",
      "/.well-known/oauth-protected-resource/v1/mcp",
      "/health",
      "/metrics",
    ].includes(path)
  ) {
    const upstream = coreUrl(path);

    upstream.search = request.nextUrl.search;

    return NextResponse.rewrite(upstream);
  }

  const nonce = Buffer.from(crypto.randomUUID()).toString("base64");
  const development = process.env.NODE_ENV === "development";
  const styles = development ? "'unsafe-inline'" : `'nonce-${nonce}'`;
  const policy = [
    "default-src 'self'",
    `script-src 'self' 'nonce-${nonce}' 'strict-dynamic'${development ? " 'unsafe-eval'" : ""}`,
    `style-src 'self' ${styles}`,
    "style-src-attr 'unsafe-inline'",
    "img-src 'self' data:",
    "font-src 'self'",
    "connect-src 'self'",
    "object-src 'none'",
    "base-uri 'none'",
    "form-action 'self'",
    "frame-ancestors 'none'",
  ].join("; ");
  const headers = new Headers(request.headers);

  headers.set("x-nonce", nonce);
  headers.set("content-security-policy", policy);

  const response = NextResponse.next({ request: { headers } });

  response.headers.set("content-security-policy", policy);
  response.headers.set("x-content-type-options", "nosniff");
  response.headers.set("referrer-policy", "no-referrer");

  return response;
}

export const config = {
  matcher: [
    "/((?!_next/static|_next/image|feeds/[^/]+/[^/]+$|ui-health$|favicon.svg$).*)",
  ],
};
