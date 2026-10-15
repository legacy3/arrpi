import { expect } from "@playwright/test";

import { mockConfigEditor, test } from "./fixtures/ui";

test("MCP activity lists access and usage, copies connections and links to settings", async ({
  configDocument: document,
  connect,
  page,
}) => {
  document.settings.clients = document.settings.clients.filter(
    (client) => !client.seerr_user,
  );
  document.settings.clients.push({
    key: null,
    name: "mcp-friend",
    permissions: ["catalog:view"],
    seerr_user: { id: 2, plex_id: 102 },
  });
  document.settings.mcp.public_url = "https://arrpi.example.test/v1/mcp";
  const { writes } = await mockConfigEditor(page, document);

  await page.route("**/v1/mcp/status?*", async (route) =>
    route.fulfill({
      json: {
        connected: true,
        tools: [
          {
            available: true,
            description: "Search the catalog",
            name: "search_media",
          },
          {
            available: false,
            description: "Cancel a pending request",
            name: "cancel_request",
          },
        ],
        writes_supported: true,
      },
    }),
  );
  await page.route("**/v1/config/mcp/stats?*", async (route) =>
    route.fulfill({
      json: {
        items: [
          {
            calls: 7,
            last_used: "2026-10-14T18:40:00Z",
            outcome: "ok",
            tool: "search_media",
          },
          {
            calls: 2,
            last_used: "2026-10-14T18:41:00Z",
            outcome: "invalid_arguments",
            tool: "search_media",
          },
        ],
      },
    }),
  );
  await page.route("**/v1/config/mcp/audit?*", async (route) =>
    route.fulfill({
      json: {
        items: [
          {
            actor_id: 2,
            created_at: "2026-10-14T18:41:00Z",
            id: 1,
            outcome: "approved",
            target: { request_id: 7, tmdb_id: 42 },
            tool: "request_movie",
          },
        ],
        next_offset: null,
        total: 1,
      },
    }),
  );
  await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
  let releaseToken: (() => void) | undefined;
  let delayed = false;
  let waiting = false;

  await page.route("**/v1/mcp/token?*", async (route) => {
    expect(new URL(route.request().url()).searchParams.get("client")).toBe(
      "mcp-friend",
    );

    if (delayed) {
      waiting = true;
      await new Promise<void>((resolve) => {
        releaseToken = resolve;
      });
    }

    await route.fulfill({
      json: { token: "fake-mcp-account-token-0000000000000000" },
    });
  });
  await connect();
  await page.getByRole("link", { exact: true, name: "MCP" }).click();
  await expect(page).toHaveURL((url) => url.pathname === "/mcp");
  await expect(
    page.getByRole("heading", { exact: true, name: "MCP" }),
  ).toBeVisible();
  await expect(page.getByLabel("Account", { exact: true })).toHaveValue(
    "mcp-friend",
  );
  await expect(
    page.getByRole("table", { name: "Write history" }),
  ).toContainText("TMDB 42");
  await expect(
    page.getByRole("table", { name: "Write history" }),
  ).toContainText("request #7");
  await expect(page.getByLabel("Enable MCP", { exact: true })).toHaveCount(0);
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("row").filter({ hasText: "cancel_request" }),
  ).toContainText("Not permitted");
  expect(
    await page.getByRole("heading", { level: 2 }).first().textContent(),
  ).toBe("Connect a client");
  await expect(page.getByLabel("Client", { exact: true })).toHaveCount(0);
  await expect(page.locator("pre")).toHaveCount(0);

  for (const [client, label] of [
    ["claude", "Claude Code"],
    ["codex", "Codex"],
    ["gemini", "Gemini CLI"],
  ]) {
    await page
      .getByRole("button", { exact: true, name: `Copy ${label}` })
      .click();
    await expect(
      page.getByRole("button", { exact: true, name: `Copy ${label}` }),
    ).toHaveText(`Copied ${label}`);
    const copied = await page.evaluate(() => navigator.clipboard.readText());

    expect(copied).toContain(client + " mcp add");
    expect(copied).toContain("fake-mcp-account-token-0000000000000000");
    expect(copied).toContain("https://arrpi.example.test/v1/mcp");
    expect(copied).not.toContain("<account token>");
    expect(copied).not.toContain("export ");
    expect(copied).not.toContain("\n");
    expect(await page.content()).not.toContain(
      "fake-mcp-account-token-0000000000000000",
    );
  }

  delayed = true;
  await page.getByRole("button", { exact: true, name: "Copy Codex" }).click();
  await expect(page.getByLabel("Account", { exact: true })).toBeDisabled();
  await expect(
    page.getByRole("button", { exact: true, name: "Copy Claude Code" }),
  ).toBeDisabled();
  await expect.poll(() => waiting).toBe(true);
  releaseToken?.();
  await expect(page.getByLabel("Account", { exact: true })).toBeEnabled();
  delayed = false;
  await page.getByLabel("Account", { exact: true }).selectOption("reader");

  for (const label of ["Claude Code", "Codex", "Gemini CLI"]) {
    await expect(
      page.getByRole("button", { exact: true, name: `Copy ${label}` }),
    ).toBeDisabled();
  }

  await page.getByLabel("Account", { exact: true }).selectOption("mcp-friend");
  await expect(
    page
      .getByRole("row")
      .filter({ hasText: "search_media" })
      .getByRole("cell")
      .nth(2),
  ).toHaveText("9");
  await expect(
    page
      .getByRole("row")
      .filter({ hasText: "search_media" })
      .getByRole("cell")
      .nth(3),
  ).toHaveText("2");
  await expect(page.getByLabel("Seerr URL", { exact: true })).toHaveCount(0);
  expect(writes).toHaveLength(0);
  await page.getByRole("link", { exact: true, name: "MCP settings" }).click();
  await expect(page).toHaveURL(/\/settings\/mcp$/);
  await page.getByLabel("Items per batch", { exact: true }).fill("30");
  await page
    .getByLabel("Write history entries per account", { exact: true })
    .fill("25000");
  await page.getByLabel("Enable search_media", { exact: true }).uncheck();
  await page.getByLabel("Enable MCP", { exact: true }).uncheck();
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText("Saved and applied.");
  await expect.poll(() => writes.length).toBe(1);
  const saved = writes.at(-1);

  expect(saved?.settings.mcp.disabled_tools).toEqual(["search_media"]);
  expect(saved?.settings.mcp.enabled).toBe(false);
  expect(saved?.settings.mcp.limits.max_batch_items).toBe(30);
  expect(saved?.settings.mcp.limits.audit_max_entries).toBe(25_000);
  expect(saved?.settings.seerr).toEqual(document.settings.seerr);
  await page
    .getByRole("navigation", { name: "Main navigation" })
    .getByRole("link", { exact: true, name: "MCP" })
    .click();
  await expect(
    page.getByRole("row").filter({ hasText: "search_media" }),
  ).toContainText("Disabled");
  const refreshed = page.waitForRequest(
    (request) =>
      new URL(request.url()).pathname === "/v1/mcp/status" &&
      request.headers()["cache-control"] === "no-cache",
  );

  await page
    .getByRole("button", { exact: true, name: "Refresh tool usage" })
    .click();
  await refreshed;

  expect(saved?.settings.clients.every((client) => client.key === null)).toBe(
    true,
  );
  expect(await page.content()).not.toContain(
    "fake-browser-key-00000000000000000000",
  );
  expect(
    await page.evaluate(
      () => globalThis.document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.evaluate(() => scrollTo({ behavior: "instant", top: 0 }));
  await page.screenshot({
    fullPage: true,
    path: `test-results/mcp-${test.info().project.name}.png`,
  });
});

test("missing Seerr connection gates MCP setup and points to general Settings", async ({
  configDocument: document,
  connect,
  page,
}) => {
  document.settings.seerr = null;
  await page.route("**/v1/config", async (route) =>
    route.fulfill({ json: document }),
  );
  await connect("/mcp");
  await expect(page.getByLabel("Enable MCP", { exact: true })).toHaveCount(0);
  await expect(
    page.getByRole("button", { exact: true, name: "Copy Codex" }),
  ).toBeDisabled();
  await page
    .getByRole("link", { name: "Seerr connection in Settings" })
    .click();
  await expect(page).toHaveURL(/\/settings$/);
  await page.getByRole("button", { exact: true, name: "Edit Seerr" }).click();
  await expect(page.getByLabel("Seerr URL", { exact: true })).toBeVisible();
});

test("MCP settings conflicts preserve edits until explicitly reloaded", async ({
  configDocument,
  connect,
  page,
}) => {
  configDocument.settings.clients.push({
    key: null,
    name: "second-account",
    permissions: ["catalog:view"],
    seerr_user: { id: 3, plex_id: 103 },
  });
  const mock = await mockConfigEditor(page, configDocument);

  await connect("/settings/mcp");
  await page.getByLabel("Items per batch", { exact: true }).fill("25");
  mock.setConflict(true);
  configDocument.revision += 1;
  configDocument.settings.mcp.limits.max_batch_items = 40;
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(
    page.getByRole("button", { exact: true, name: "Reload settings" }),
  ).toBeVisible();
  await expect(page.getByLabel("Items per batch", { exact: true })).toHaveValue(
    "25",
  );
  await page
    .getByRole("button", { exact: true, name: "Reload settings" })
    .click();
  await expect(page.getByLabel("Items per batch", { exact: true })).toHaveValue(
    "40",
  );
});

test("MCP settings validate inputs and preserve unrelated configuration", async ({
  configDocument: document,
  connect,
  page,
}) => {
  const original = structuredClone(document.settings);
  const { writes } = await mockConfigEditor(page, document);

  await connect("/settings/mcp");
  await expect(
    page.getByRole("heading", { exact: true, name: "MCP server" }),
  ).toBeVisible();
  await expect(page.getByLabel("Account", { exact: true })).toHaveCount(0);
  await expect(page.getByRole("table", { name: "Write history" })).toHaveCount(
    0,
  );

  await page.getByLabel("Public endpoint", { exact: true }).fill("invalid url");
  await page.getByLabel("Public endpoint", { exact: true }).blur();
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toBeDisabled();
  await page
    .getByLabel("Public endpoint", { exact: true })
    .fill("https://arrpi.example.test/wrong");
  await expect(page.getByRole("main").getByRole("alert")).toContainText(
    "HTTPS /v1/mcp",
  );
  await page
    .getByLabel("Public endpoint", { exact: true })
    .fill("https://arrpi.example.test/v1/mcp");
  await page.getByLabel("Items per batch", { exact: true }).fill("101");
  await page.getByLabel("Items per batch", { exact: true }).blur();
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toBeDisabled();
  expect(writes).toHaveLength(0);
  await page.getByLabel("Items per batch", { exact: true }).fill("35");

  for (const invalid of [
    "https://host/v1/mcp?x=1",
    "https://host/v1/mcp#frag",
    "https://user:pw@host/v1/mcp",
  ]) {
    await page.getByLabel("Public endpoint", { exact: true }).fill(invalid);
    await expect(
      page.getByRole("button", { exact: true, name: "Save" }),
    ).toBeDisabled();
  }

  await page
    .getByLabel("Public endpoint", { exact: true })
    .fill("https://arrpi.example.test/v1/mcp");
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText("Saved and applied.");
  const saved = writes.at(-1)?.settings;

  expect(saved?.mcp.limits.max_batch_items).toBe(35);
  expect(saved?.clients).toEqual(original.clients);
  expect(saved?.sources).toEqual(original.sources);
  expect(saved?.seerr).toEqual(original.seerr);
  expect(saved?.feed).toEqual(original.feed);
  expect(saved?.subscriptions).toEqual(original.subscriptions);
  await page.evaluate(() => scrollTo({ behavior: "instant", top: 0 }));
  await page.screenshot({
    fullPage: true,
    path: `test-results/mcp-settings-${test.info().project.name}.png`,
  });
});

test("MCP settings explain missing Seerr and disable the master switch", async ({
  configDocument: document,
  connect,
  page,
}) => {
  document.settings.seerr = null;
  document.settings.mcp.enabled = false;
  const { writes } = await mockConfigEditor(page, document);

  await connect("/settings/mcp");
  await expect(page.getByLabel("Enable MCP", { exact: true })).toBeDisabled();
  await page
    .getByRole("link", { name: "Seerr connection in Settings" })
    .click();
  await expect(page).toHaveURL(/\/settings$/);
  document.settings.mcp.enabled = true;
  document.revision += 1;
  await connect("/settings/mcp");
  await expect(page.getByLabel("Enable MCP", { exact: true })).toBeChecked();
  await page.getByLabel("Enable MCP", { exact: true }).uncheck();
  await expect(page.getByLabel("Enable MCP", { exact: true })).toBeDisabled();
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText("Saved and applied.");
  expect(writes.at(-1)?.settings.mcp.enabled).toBe(false);
});

test("MCP settings remain editable when the tool registry is unavailable", async ({
  configDocument: document,
  connect,
  page,
}) => {
  document.settings.mcp.disabled_tools = ["upcoming_releases"];
  const { writes } = await mockConfigEditor(page, document);

  await page.route("**/v1/mcp/status?*", (route) =>
    route.fulfill({ json: { error: "upstream_unavailable" }, status: 503 }),
  );
  await connect("/settings/mcp");
  await expect(page.getByRole("main").getByRole("alert")).toBeVisible();
  await page.getByLabel("Items per batch", { exact: true }).fill("25");
  await page.getByLabel("Enable MCP", { exact: true }).uncheck();
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(
    page.locator('[data-slot="form-actions"]').getByRole("status"),
  ).toHaveText("Saved and applied.");
  expect(writes.at(-1)?.settings.mcp.disabled_tools).toEqual([
    "upcoming_releases",
  ]);
  expect(writes.at(-1)?.settings.mcp.enabled).toBe(false);
  expect(writes.at(-1)?.settings.mcp.limits.max_batch_items).toBe(25);
});

test("a late registry preserves edits and disabled tools outside its response", async ({
  configDocument: document,
  connect,
  page,
}) => {
  document.settings.mcp.disabled_tools = ["upcoming_releases"];
  const { writes } = await mockConfigEditor(page, document);
  let release: (() => void) | undefined;

  await page.route("**/v1/mcp/status?*", async (route) => {
    await new Promise<void>((resolve) => {
      release = resolve;
    });
    await route.fulfill({
      json: {
        connected: true,
        tools: [
          {
            available: true,
            description: "Search the catalog",
            name: "search_media",
          },
        ],
        writes_supported: true,
      },
    });
  });
  await connect("/settings/mcp");
  await page.getByLabel("Items per batch", { exact: true }).fill("28");
  await expect.poll(() => Boolean(release)).toBe(true);
  release?.();
  await expect(
    page.getByLabel("Enable search_media", { exact: true }),
  ).toBeChecked();
  await expect(page.getByLabel("Items per batch", { exact: true })).toHaveValue(
    "28",
  );
  await page.getByLabel("Enable search_media", { exact: true }).uncheck();
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText("Saved and applied.");
  expect(writes.at(-1)?.settings.mcp.disabled_tools).toEqual([
    "upcoming_releases",
    "search_media",
  ]);
  expect(writes.at(-1)?.settings.mcp.limits.max_batch_items).toBe(28);
});
