import { writeFileSync } from "node:fs";
import { createServer } from "node:http";

createServer((request, response) => {
  if (request.url?.startsWith("/v1/calendar.rss")) {
    response.writeHead(200, { "Content-Type": "application/rss+xml" });
    response.flushHeaders();
    setTimeout(() => response.end("streamed-feed"), 21_000);

    return;
  }

  const slow = request.url?.includes("slow") ?? false;

  if (!slow) {
    writeFileSync("/run/proxy-test/draining", "1");
  }

  setTimeout(
    () => response.end(slow ? "slow" : "drained"),
    slow ? 31_000 : 2000,
  );
}).listen(8780, "127.0.0.1");
