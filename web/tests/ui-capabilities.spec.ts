import { expect, type Page } from "@playwright/test";

import { test } from "./fixtures/ui";

const requester = "fake-requester-key-1111111111111111111";

async function choose(page: Page, label: string, option: string) {
  await page.getByRole("combobox", { exact: true, name: label }).click();
  await page.getByRole("option", { exact: true, name: option }).click();
}

test("Explorer scopes maintenance commands and presets the selected source", async ({
  connect,
  page,
}) => {
  await connect("/explorer");
  await choose(page, "Source", "sonarr");
  await choose(page, "Operation", "refresh_media");
  await expect(page.getByLabel("Arguments (JSON)")).toHaveValue(
    /"source": "sonarr"/,
  );
  await expect(
    page.getByText("Writes to sonarr", { exact: true }),
  ).toBeVisible();
  await choose(page, "Source", "radarr");
  await choose(page, "Operation", "refresh_media");
  await expect(page.getByLabel("Arguments (JSON)")).toHaveValue(
    /"source": "radarr"/,
  );
  await expect(
    page.getByText("Writes to radarr", { exact: true }),
  ).toBeVisible();
  await page.getByRole("combobox", { exact: true, name: "Operation" }).click();
  await expect(
    page.getByRole("option", { exact: true, name: "series_episodes" }),
  ).toHaveCount(0);
});

test("overview includes every installed source and normalized operational data", async ({
  connect,
  page,
}, info) => {
  await connect();
  const sources = page.locator('[data-slot="card"]').filter({
    has: page.getByRole("heading", { exact: true, name: "Sources" }),
  });

  for (const name of [
    "sonarr",
    "seerr",
    "radarr",
    "sabnzbd",
    "cleanuparr",
    "bazarr",
    "maintainerr",
  ]) {
    await expect(
      sources.getByRole("row").filter({ hasText: name }),
    ).toBeVisible();
  }

  const monitoring = page.locator('[data-slot="card"]').filter({
    has: page.getByRole("heading", { exact: true, name: "Monitoring" }),
  });

  await expect(
    monitoring.getByRole("row").filter({ hasText: "sabnzbd" }),
  ).toContainText("2 MiB/s");
  await expect(
    monitoring.getByRole("row").filter({ hasText: "bazarr" }),
  ).toContainText("Episode subtitle requirements12");
  await expect(
    monitoring.getByRole("row").filter({ hasText: "cleanuparr" }),
  ).toContainText("Removed in 7 days8");
  await expect(
    monitoring.getByRole("row").filter({ hasText: "maintainerr" }),
  ).toContainText("Collection sizes incomplete or overlapping");
  await expect(
    page.getByText("Unmeasured library sizes", { exact: true }),
  ).toHaveCount(2);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    fullPage: true,
    path: info.outputPath("all-connections.png"),
  });
});

test("Explorer reads monitoring through the real summary API with a fresh-read option", async ({
  connect,
  page,
}) => {
  await connect("/explorer");
  await choose(page, "Source", "sabnzbd");
  await page
    .getByRole("checkbox", { exact: true, name: "Fresh upstream read" })
    .check();
  const response = page.waitForResponse(
    (item) =>
      item.url().includes("/v1/sources/sabnzbd/summaries/status") &&
      item.request().headers()["cache-control"] === "no-cache",
  );

  await page.getByRole("button", { exact: true, name: "Fetch" }).click();
  expect((await response).ok()).toBe(true);
  await expect(page.getByLabel("JSON response", { exact: true })).toHaveValue(
    /"bypass_requested": true/,
  );
  await expect(page.getByLabel("JSON response", { exact: true })).toHaveValue(
    /"source": "sabnzbd"/,
  );
});

test("Explorer offers every connection directly and clears results when the source changes", async ({
  connect,
  page,
}) => {
  await connect("/explorer");
  await expect(
    page.getByRole("combobox", { exact: true, name: "Interface" }),
  ).toHaveCount(0);
  await page.getByRole("combobox", { exact: true, name: "Source" }).click();

  for (const name of [
    "All sources",
    "sonarr",
    "radarr",
    "seerr",
    "sabnzbd",
    "cleanuparr",
    "bazarr",
    "maintainerr",
  ]) {
    await expect(page.getByRole("option", { exact: true, name })).toBeVisible();
  }

  await page.keyboard.press("Escape");

  for (const name of ["sabnzbd", "cleanuparr", "bazarr", "maintainerr"]) {
    await choose(page, "Source", name);
    await expect(page.getByLabel("JSON response", { exact: true })).toHaveCount(
      0,
    );
    const response = page.waitForResponse(
      (response) =>
        new URL(response.url()).pathname ===
        `/v1/sources/${name}/summaries/status`,
    );

    await page.getByRole("button", { exact: true, name: "Fetch" }).click();
    const data = (await (await response).json()) as {
      sources: { source: string }[];
    };

    expect(data.sources.map((source) => source.source)).toEqual([name]);
    await expect(page.getByLabel("JSON response", { exact: true })).toHaveValue(
      new RegExp(`"source": "${name}"`),
    );
  }

  await choose(page, "Source", "seerr");
  await expect(
    page.getByText(
      "A requester account must be linked before commands are available.",
      { exact: true },
    ),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { exact: true, name: "Run command" }),
  ).toHaveCount(0);
});

test("overview refresh bypasses each authorized operational cache", async ({
  connect,
  page,
}) => {
  await connect();
  const refresh = page.getByRole("button", { name: "Refresh cache status" });

  await expect(refresh).toBeEnabled();
  const responses = ["library", "queue", "status"].map((kind) =>
    page.waitForResponse(
      (response) =>
        response.url().endsWith(`/v1/summaries/${kind}`) &&
        response.request().headers()["cache-control"] === "no-cache",
    ),
  );

  await refresh.click();

  for (const response of responses) {
    const result = await response;

    expect(result.ok()).toBe(true);
    expect(await result.text()).toContain('"bypass_requested":true');
  }
});

test("Explorer does not display Overview's cached response before Fetch", async ({
  connect,
  page,
}) => {
  await connect("/explorer");
  await page.getByRole("link", { exact: true, name: "Overview" }).click();
  await expect(
    page.getByRole("heading", { exact: true, name: "Library" }),
  ).toBeVisible();
  await page.waitForLoadState("networkidle");
  await page.getByRole("link", { exact: true, name: "Explorer" }).click();
  await expect(
    page.getByRole("heading", { exact: true, name: "No response yet" }),
  ).toBeVisible();
  await expect(page.getByLabel("JSON response", { exact: true })).toHaveCount(
    0,
  );
  await page.getByLabel("Fresh upstream read", { exact: true }).check();
  const response = page.waitForResponse(
    (response) =>
      new URL(response.url()).pathname === "/v1/summaries/library" &&
      response.request().headers()["cache-control"] === "no-cache",
  );

  await page.getByRole("button", { exact: true, name: "Fetch" }).click();
  expect((await response).ok()).toBe(true);
  await expect(page.getByLabel("JSON response", { exact: true })).toHaveValue(
    /"bypass_requested": true/,
  );
});

test("disabled account commands do not break individual source reads", async ({
  connect,
  page,
}) => {
  await page.route("**/v1/tools", (route) =>
    route.fulfill({ json: { error: "not_found" }, status: 404 }),
  );
  await connect("/explorer");
  await choose(page, "Source", "sabnzbd");
  await page.getByRole("button", { exact: true, name: "Fetch" }).click();
  await expect(page.getByLabel("JSON response", { exact: true })).toHaveValue(
    /"source": "sabnzbd"/,
  );
  await expect(page.getByRole("main").getByRole("alert")).toHaveCount(0);
  await choose(page, "Source", "seerr");
  await expect(
    page.getByText("Enable MCP in Settings to use account commands.", {
      exact: true,
    }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { exact: true, name: "Run command" }),
  ).toHaveCount(0);
});

test("Explorer discovers account commands and passes multiword inputs without native URL construction", async ({
  connect,
  page,
}) => {
  await connect("/explorer", requester);
  await expect(
    page.getByRole("combobox", { exact: true, name: "Source" }),
  ).toBeVisible();
  await choose(page, "Source", "seerr");
  await choose(page, "Operation", "search_media");
  await page
    .getByLabel("Arguments (JSON)", { exact: true })
    .fill('{"query":"Moonberry Express"}');
  await page
    .getByRole("checkbox", { exact: true, name: "Fresh upstream read" })
    .check();
  const request = page.waitForRequest(
    (item) =>
      item.url().endsWith("/v1/tools/search_media") && item.method() === "POST",
  );

  await page.getByRole("button", { exact: true, name: "Run command" }).click();
  expect((await request).postDataJSON()).toEqual({
    no_cache: true,
    query: "Moonberry Express",
  });
  await expect(page.getByLabel("JSON response", { exact: true })).toHaveValue(
    /Moonberry Express/,
  );
  await expect(page.getByLabel("JSON response", { exact: true })).toHaveValue(
    /"tmdb_id": 42/,
  );
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
});

test("Explorer writes require an explicit account confirmation and never retry unknown outcomes", async ({
  connect,
  page,
}) => {
  const writes: unknown[] = [];

  await page.route("**/v1/tools/request_movie", async (route) => {
    writes.push(route.request().postDataJSON());
    await route.fulfill({
      json: { code: "unknown_outcome", error: "mutation_outcome_unknown" },
      status: 502,
    });
  });
  await connect("/explorer", requester);
  await choose(page, "Source", "seerr");
  await choose(page, "Operation", "request_movie");
  const run = page.getByRole("button", { exact: true, name: "Run command" });
  const confirmation = page.getByRole("checkbox", {
    name: "Apply this change to my account",
  });

  await expect(run).toBeDisabled();
  await page
    .getByLabel("Arguments (JSON)", { exact: true })
    .fill('{"tmdb_id":42}');
  await confirmation.check();
  await page
    .getByLabel("Arguments (JSON)", { exact: true })
    .fill('{"tmdb_id": 42}');
  await expect(run).toBeDisabled();
  await confirmation.check();
  await run.click();
  await expect(page.getByRole("main").getByRole("alert")).toHaveText(
    "mutation_outcome_unknown",
  );
  await expect.poll(() => writes.length).toBe(1);
  await expect(run).toBeDisabled();
  await choose(page, "Operation", "my_quota");
  await expect(page.getByRole("main").getByRole("alert")).toHaveCount(0);
  expect(writes).toEqual([{ tmdb_id: 42 }]);
});

test("Explorer rejects non-object arguments before sending a command", async ({
  connect,
  page,
}) => {
  const writes: string[] = [];

  page.on("request", (request) => {
    if (request.method() === "POST") {
      writes.push(request.url());
    }
  });
  await connect("/explorer", requester);
  await choose(page, "Source", "seerr");
  await choose(page, "Operation", "search_media");
  await page.getByLabel("Arguments (JSON)", { exact: true }).fill("[]");
  await page.getByRole("button", { exact: true, name: "Run command" }).click();
  await expect(page.getByRole("main").getByRole("alert")).toHaveText(
    "Arguments must be a JSON object",
  );
  expect(writes).toHaveLength(0);
});

test("one failed monitoring source retains healthy rows and a failed refresh labels previous results", async ({
  connect,
  page,
}) => {
  await page.route("**/v1/summaries/status", async (route) => {
    const response = await route.fetch();
    const document = (await response.json()) as {
      complete: boolean;
      sources: { source: string }[];
    };

    await route.fulfill({
      json: {
        ...document,
        complete: false,
        sources: document.sources.map((row) =>
          row.source === "bazarr"
            ? {
                code: "upstream_timeout",
                kind: "bazarr",
                source: "bazarr",
                status: "unavailable",
              }
            : row,
        ),
      },
    });
  });
  await connect();
  const monitoring = page.locator('[data-slot="card"]').filter({
    has: page.getByRole("heading", { exact: true, name: "Monitoring" }),
  });

  await expect(
    monitoring.getByText("Unavailable · upstream_timeout", { exact: true }),
  ).toBeVisible();
  await expect(monitoring.getByText("2 MiB/s", { exact: true })).toBeVisible();
  await page.route("**/v1/summaries/status", (route) =>
    route.fulfill({ json: { error: "upstream_unavailable" }, status: 503 }),
  );
  await page
    .getByRole("button", { name: "Refresh monitoring from upstream" })
    .click();
  await expect(monitoring.getByRole("alert")).toHaveText(
    "upstream_unavailable · previous results",
  );
  await expect(monitoring.getByText("2 MiB/s", { exact: true })).toBeVisible();
});

test("calendar-only access never requests operational projections or shows monitoring connections", async ({
  connect,
  page,
}) => {
  const reads: string[] = [];

  page.on("request", (request) => {
    if (request.url().includes("/v1/summaries/")) {
      reads.push(request.url());
    }
  });
  await connect("/", "fake-calendar-key-0000000000000000000");
  await expect(
    page.getByRole("heading", { exact: true, name: "Sources" }),
  ).toBeVisible();
  await expect(page.getByText("sabnzbd", { exact: true })).toHaveCount(0);
  await page.getByRole("link", { exact: true, name: "Explorer" }).click();
  await choose(page, "Operation", "upcoming_releases");
  await expect(
    page.getByRole("combobox", { exact: true, name: "Source" }),
  ).toBeVisible();
  expect(reads).toHaveLength(0);
});
