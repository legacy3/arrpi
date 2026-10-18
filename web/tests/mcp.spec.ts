import { expect, test } from "@playwright/test";
import { spawn } from "node:child_process";

const key = "fake-browser-key-00000000000000000000";
const headers = {
  Accept: "application/json, text/event-stream",
  Authorization: `Bearer ${key}`,
  "MCP-Protocol-Version": "2025-06-18",
};
const initialize = {
  capabilities: {},
  clientInfo: { name: "fixture", version: "1" },
  protocolVersion: "2025-06-18",
};

for (const origin of ["http://127.0.0.1:4173", "http://127.0.0.1:4174"]) {
  test(`MCP negotiates and reads through ${origin}`, async ({ request }) => {
    const init = await request.post(`${origin}/v1/mcp`, {
      data: { id: 1, jsonrpc: "2.0", method: "initialize", params: initialize },
      headers,
    });

    expect(init.status()).toBe(200);
    expect(await init.json()).toMatchObject({
      result: { serverInfo: { name: "arrpi" } },
    });
    const catalog = await request.post(`${origin}/v1/mcp`, {
      data: { id: 3, jsonrpc: "2.0", method: "tools/list" },
      headers,
    });
    const definitions = (await catalog.json()) as {
      result: { tools: { name: string }[] };
    };

    expect(definitions.result.tools.map((tool) => tool.name)).toEqual(
      expect.arrayContaining([
        "refresh_media",
        "search_releases",
        "series_episodes",
        "media_command_status",
      ]),
    );
    const result = await request.post(`${origin}/v1/mcp`, {
      data: {
        id: 2,
        jsonrpc: "2.0",
        method: "tools/call",
        params: { arguments: { limit: 5 }, name: "list_series" },
      },
      headers,
    });

    expect(result.status()).toBe(200);
    const data: unknown = await result.json();

    expect(data).toMatchObject({
      result: {
        structuredContent: {
          items: [
            { title: "Moonberry Station" },
            { title: "The Midnight Book Club" },
          ],
        },
      },
    });
    expect(JSON.stringify(data)).not.toContain("/private");
    expect(
      (await request.post(`${origin}/v1/mcp`, { data: {} })).status(),
    ).toBe(401);
  });
}

test("stdio emits only MCP messages and shares the HTTP command API", async () => {
  const child = spawn("/usr/bin/sg", [
    "docker",
    "-c",
    "docker exec -i -e ARRPI_API_KEY=fake-browser-key-00000000000000000000 arrpi-test-arrpi-1 arrpi mcp --url http://127.0.0.1:8780",
  ]);
  const pending = new Map<number, (value: unknown) => void>();
  let buffered = "";

  child.stdout.setEncoding("utf8");
  child.stdout.on("data", (chunk: string) => {
    buffered += chunk;
    const lines = buffered.split("\n");

    buffered = lines.pop() ?? "";

    for (const line of lines) {
      const message = JSON.parse(line) as { id?: number };

      if (message.id !== undefined) {
        pending.get(message.id)?.(message);
        pending.delete(message.id);
      }
    }
  });
  const send = (id: number, method: string, params: unknown) =>
    new Promise<unknown>((resolve) => {
      pending.set(id, resolve);
      child.stdin.write(
        JSON.stringify({ id, jsonrpc: "2.0", method, params }) + "\n",
      );
    });

  try {
    expect(await send(1, "initialize", initialize)).toMatchObject({
      result: { serverInfo: { name: "arrpi" } },
    });
    child.stdin.write(
      '{"jsonrpc":"2.0","method":"notifications/initialized"}\n',
    );
    expect(
      await send(2, "tools/call", {
        arguments: { limit: 1 },
        name: "list_movies",
      }),
    ).toMatchObject({
      result: { structuredContent: { total: 4 } },
    });
  } finally {
    child.stdin.end();
    child.kill();
  }
});

for (const origin of ["http://127.0.0.1:4173", "http://127.0.0.1:4174"]) {
  test(`structured library search through ${origin}`, async ({ request }) => {
    const response = await request.post(`${origin}/v1/mcp`, {
      data: {
        id: 1,
        jsonrpc: "2.0",
        method: "tools/call",
        params: {
          arguments: {
            available: true,
            genres: ["Comedy"],
            limit: 5,
            media_type: "movie",
            min_rating: 7,
            sort: "rating",
          },
          name: "search_library",
        },
      },
      headers,
    });

    expect(response.status()).toBe(200);
    expect(await response.json()).toMatchObject({
      result: {
        structuredContent: {
          complete: true,
          items: [
            {
              overview:
                "A lighthouse keeper and a travelling baker try to save their town’s winter festival.",
              rating: 8.2,
              rating_provider: "imdb",
              runtime_minutes: 110,
              title: "The Last Lantern",
            },
            { rating: 7.8, runtime_minutes: 96, title: "Captain Chamomile" },
            { rating: 7.4, runtime_minutes: 88, title: "Parcel Cats" },
          ],
          next_offset: null,
          total: 3,
        },
      },
    });
  });
}
