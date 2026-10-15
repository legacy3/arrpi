import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  // Core allows queue/batch deadlines of 300 seconds each.
  experimental: { proxyClientMaxBodySize: "128kb", proxyTimeout: 660_000 },
  images: { unoptimized: true },
  output: "standalone",
  outputFileTracingExcludes: {
    "**": [
      "**/node_modules/.pnpm/sharp@*/**/*",
      "**/node_modules/.pnpm/@img+sharp*/**/*",
    ],
  },
  poweredByHeader: false,
  skipTrailingSlashRedirect: true,
};

export default nextConfig;
