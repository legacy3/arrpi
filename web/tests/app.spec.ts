import { expect, type Page, test } from "@playwright/test";

import type { Calendar, Status } from "../src/lib/api";

const key = "fake-browser-key-00000000000000000000";
const status: Status = {
  cache: {
    connection_idle_seconds: 30,
    connection_lifetime_seconds: 300,
    fresh_seconds: 60,
    max_body_bytes: 1_048_576,
    max_connections: 64,
    max_entries: 32,
    max_normalizations: 4,
    max_requests: 64,
    max_retained_bytes: 134_217_728,
    max_upstream_requests: 4,
    retry_seconds: 5,
    stale_seconds: 900,
    timeout_seconds: 10,
  },
  config_manage: false,
  sources: [
    {
      account: false,
      calendar: true,
      counters: { hits: 35, misses: 5, stale: 0, upstream_failures: 0 },
      endpoints: ["calendar", "series", "queue", "health"],
      kind: "sonarr",
      name: "sonarr",
      summaries: [],
    },
    {
      account: false,
      calendar: true,
      counters: { hits: 12, misses: 3, stale: 1, upstream_failures: 2 },
      endpoints: ["calendar", "movie", "queue", "health"],
      kind: "radarr",
      name: "radarr",
      summaries: [],
    },
  ],
  version: "0.1.2",
};
const calendar: Calendar = {
  events: [
    {
      date: "2026-10-15T21:00:00Z",
      episode: 7,
      has_file: false,
      id: 1,
      kind: "episode",
      season: 2,
      series: "Eine ziemlich gute Serie",
      source: "sonarr",
      title: 'ÄÖÜ & <img src="x" onerror="window.pwned=true">',
    },
    {
      date: "2026-10-16",
      has_file: true,
      id: 2,
      kind: "digital",
      source: "radarr",
      title: "The Last Lantern",
      tmdb_id: 99,
    },
  ],
  sources: [
    { age_seconds: 12, cache: "hit", source: "sonarr", status: "ok" },
    { age_seconds: 123, cache: "stale", source: "radarr", status: "ok" },
  ],
};

async function chooseSource(page: Page) {
  await page.getByRole("combobox", { exact: true, name: "Source" }).click();
  await page.getByRole("option", { exact: true, name: "sonarr" }).click();
}

async function connect(page: Page) {
  await page.getByLabel("Client key").fill(key);
  await page.getByRole("button", { exact: true, name: "Connect" }).click();
  await expect(
    page.getByRole("heading", { exact: true, name: "Overview" }),
  ).toBeVisible();
}

async function fixtures(page: Page, limited = false) {
  await page.clock.install({ time: new Date("2026-10-13T12:00:00Z") });
  await page.route("**/v1/**", async (route) => {
    if (route.request().headers().authorization !== `Bearer ${key}`) {
      await route.fulfill({
        json: { error: "authentication required" },
        status: 401,
      });

      return;
    }

    const path = new URL(route.request().url()).pathname;

    switch (path) {
      case "/v1/calendar": {
        await route.fulfill({ json: calendar });

        break;
      }

      case "/v1/status": {
        await route.fulfill({
          json: limited
            ? {
                ...status,
                sources: status.sources.map((source) => ({
                  ...source,
                  endpoints: [],
                })),
              }
            : status,
        });

        break;
      }

      case "/v1/tools": {
        await route.fulfill({ json: { account: null, tools: [] } });

        break;
      }

      default: {
        await route.fulfill({
          headers: {
            age: "0",
            "x-arrpi-cache":
              route.request().headers()["cache-control"] === "no-cache"
                ? "miss"
                : "hit",
          },
          json: [{ id: 1, title: "Native response", unknown: true }],
        });
      }
    }
  });
  await page.goto("/");
}

test("connects, shows scoped overview, clears secrets on disconnect", async ({
  page,
}, info) => {
  await fixtures(page);
  await connect(page);
  await expect(page.getByText("85%", { exact: true })).toBeVisible();
  expect(
    await page.evaluate(() => ({
      local: { ...localStorage },
      session: { ...sessionStorage },
    })),
  ).toEqual({ local: {}, session: {} });
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  await page.mouse.move(0, 0);
  await page.screenshot({
    fullPage: true,
    path: info.outputPath("overview.png"),
  });
  await page.getByRole("button", { name: "Disconnect" }).click();
  await expect(page.getByLabel("Client key")).toHaveValue("");
  await expect(page.getByText("sonarr", { exact: true })).toHaveCount(0);
});

test("calendar shows freshness, filters, escapes titles and refreshes upstream", async ({
  page,
}, info) => {
  await fixtures(page);
  await connect(page);
  await page.getByRole("link", { exact: true, name: "Calendar" }).click();
  await expect(page).toHaveURL((url) => url.pathname === "/calendar");
  await expect(
    page.getByText("Eine ziemlich gute Serie", { exact: true }),
  ).toBeVisible();
  await expect(page.getByText("stale · 123s", { exact: true })).toBeVisible();
  expect(await page.evaluate(() => "pwned" in window)).toBe(false);
  await expect(page.locator(".event-list img")).toHaveCount(0);
  await page.getByLabel("Search events").fill("Film");
  await expect(
    page.getByText("Eine ziemlich gute Serie", { exact: true }),
  ).toHaveCount(0);
  await page.getByLabel("Search events").fill("");
  const fresh = page.waitForRequest(
    (request) =>
      request.url().includes("/v1/calendar") &&
      request.headers()["cache-control"] === "no-cache",
  );

  await page
    .getByRole("button", { name: "Refresh calendar from upstream" })
    .click();
  await fresh;
  await expect(
    page.getByText("The Last Lantern", { exact: true }),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    fullPage: true,
    path: info.outputPath("calendar.png"),
  });
});

test("explorer fetches native JSON with optional cache bypass", async ({
  page,
}, info) => {
  await fixtures(page);
  await connect(page);
  await page.getByRole("link", { exact: true, name: "Explorer" }).click();
  await expect(page).toHaveURL((url) => url.pathname === "/explorer");
  await chooseSource(page);
  await page.getByRole("combobox", { exact: true, name: "Operation" }).click();
  await page.getByRole("option", { exact: true, name: "series" }).click();
  await page.getByLabel("Fresh upstream read").check();
  const fresh = page.waitForRequest(
    (request) =>
      request.url().includes("/api/v3/series") &&
      request.headers()["cache-control"] === "no-cache",
  );

  await page.getByRole("button", { exact: true, name: "Fetch" }).click();
  await fresh;
  await expect(
    page.getByRole("textbox", { name: "Native JSON response" }),
  ).toHaveValue(/Native response/);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= window.innerWidth,
    ),
  ).toBe(true);
  await page.mouse.move(0, 0);
  await page.screenshot({
    fullPage: true,
    path: info.outputPath("explorer.png"),
  });
});

test("calendar-only key has no native explorer controls", async ({ page }) => {
  await fixtures(page, true);
  await connect(page);
  await page.getByRole("link", { exact: true, name: "Explorer" }).click();
  await expect(page).toHaveURL((url) => url.pathname === "/explorer");
  await expect(
    page.getByRole("heading", { name: "No permitted operations" }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { exact: true, name: "Fetch" }),
  ).toHaveCount(0);
});

test("failed connection produces a recoverable error without private data", async ({
  page,
}) => {
  await fixtures(page);
  await page
    .getByLabel("Client key")
    .fill("invalid-browser-key-00000000000000000000");
  await page.getByRole("button", { exact: true, name: "Connect" }).click();
  await expect(page.locator("main [role=alert]")).toHaveText(
    "authentication required",
  );
  await expect(page.getByLabel("Client key")).toHaveValue("");
  await connect(page);
});

test("expired key clears the session and private views", async ({ page }) => {
  await fixtures(page);
  await connect(page);
  await page.route("**/v1/status", (route) =>
    route.fulfill({ body: "", contentType: "text/plain", status: 401 }),
  );
  await page.getByRole("button", { name: "Refresh cache status" }).click();
  await expect(page.getByLabel("Client key")).toBeVisible();
  await expect(page.locator("main [role=alert]")).toHaveText(
    "authentication required",
  );
  await expect(page.getByText("sonarr", { exact: true })).toHaveCount(0);
});

test("failed calendar refresh keeps explicitly labeled previous results", async ({
  page,
}) => {
  await fixtures(page);
  await connect(page);
  await page.getByRole("link", { exact: true, name: "Calendar" }).click();
  await expect(page).toHaveURL((url) => url.pathname === "/calendar");
  await expect(
    page.getByText("The Last Lantern", { exact: true }),
  ).toBeVisible();
  await page.route("**/v1/calendar*", (route) =>
    route.fulfill({
      body: "<html>Proxy unavailable</html>",
      contentType: "text/html",
      status: 502,
    }),
  );
  await page
    .getByRole("button", { name: "Refresh calendar from upstream" })
    .click();
  await expect(page.locator("main [role=alert]")).toHaveText(
    "Request failed (502) · previous results",
  );
  await expect(
    page.getByText("The Last Lantern", { exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Next month" }).click();
  await expect(page.getByText("The Last Lantern", { exact: true })).toHaveCount(
    0,
  );
});

test("all unavailable calendars show source failures instead of a false empty month", async ({
  page,
}) => {
  await fixtures(page);
  await connect(page);
  await page.route("**/v1/calendar*", (route) =>
    route.fulfill({
      json: {
        events: [],
        sources: [{ source: "sonarr", status: "unavailable" }],
      },
      status: 502,
    }),
  );
  await page.getByRole("link", { exact: true, name: "Calendar" }).click();
  await expect(page).toHaveURL((url) => url.pathname === "/calendar");
  await expect(page.getByText("Unavailable", { exact: true })).toBeVisible();
  await expect(page.locator("main [role=alert]")).toHaveText(
    "Request failed (502)",
  );
  await expect(
    page.getByRole("heading", { name: "No releases this month" }),
  ).toHaveCount(0);
});

test("empty month input recovers with navigation and queue page is required", async ({
  page,
}) => {
  await fixtures(page);
  await connect(page);
  await page.getByRole("link", { exact: true, name: "Calendar" }).click();
  await expect(page).toHaveURL((url) => url.pathname === "/calendar");
  await page.getByLabel("Month", { exact: true }).fill("");
  await expect(page.locator("main [role=alert]")).toHaveText("Choose a month");
  await page.getByRole("button", { name: "Previous month" }).click();
  await expect(page.getByLabel("Month", { exact: true })).toHaveValue(
    "2026-09",
  );
  await page.getByRole("link", { exact: true, name: "Explorer" }).click();
  await expect(page).toHaveURL((url) => url.pathname === "/explorer");
  await chooseSource(page);
  await page.getByRole("combobox", { exact: true, name: "Operation" }).click();
  await page.getByRole("option", { exact: true, name: "queue" }).click();
  await expect(
    page.getByRole("combobox", { exact: true, name: "Operation" }),
  ).toBeFocused();
  const pageInput = page.getByLabel("Page", { exact: true });

  await pageInput.fill("");
  await expect(pageInput).toHaveValue("");
  await expect(pageInput).toHaveJSProperty("required", true);
  await page.getByRole("button", { exact: true, name: "Fetch" }).click();
  await expect
    .poll(() =>
      pageInput.evaluate(
        (element: HTMLInputElement) => element.validity.valueMissing,
      ),
    )
    .toBe(true);
  await expect(
    page.getByRole("textbox", { name: "Native JSON response" }),
  ).toHaveCount(0);
});

test("calendar rejects year zero and navigates early years without remapping", async ({
  page,
}) => {
  const errors: string[] = [];

  page.on("pageerror", (error) => errors.push(error.message));
  await fixtures(page);
  await connect(page);
  await page.getByRole("link", { exact: true, name: "Calendar" }).click();
  await expect(page).toHaveURL((url) => url.pathname === "/calendar");
  await expect(
    page.getByText("The Last Lantern", { exact: true }),
  ).toBeVisible();
  const month = page.getByLabel("Month", { exact: true });

  await month.evaluate((element) => element.setAttribute("type", "text"));
  await month.fill("0000-01");
  await expect(page.locator("main [role=alert]")).toHaveText("Choose a month");
  await month.fill("0099-12");
  await page.getByRole("button", { name: "Next month" }).click();
  await expect(month).toHaveValue("0100-01");
  expect(errors).toEqual([]);
});

test("fresh preference changes no data until Fetch is pressed", async ({
  page,
}) => {
  const calls: string[] = [];

  page.on("request", (request) => {
    if (request.url().includes("/api/v3/series")) {
      calls.push(request.headers()["cache-control"] ?? "cached");
    }
  });
  await fixtures(page);
  await connect(page);
  await page.getByRole("link", { exact: true, name: "Explorer" }).click();
  await expect(page).toHaveURL((url) => url.pathname === "/explorer");
  await chooseSource(page);
  await page.getByRole("combobox", { exact: true, name: "Operation" }).click();
  await page.getByRole("option", { exact: true, name: "series" }).click();
  await page.getByRole("button", { exact: true, name: "Fetch" }).click();
  await expect(
    page.getByRole("textbox", { name: "Native JSON response" }),
  ).toHaveValue(/Native response/);
  await page.getByLabel("Fresh upstream read").check();
  await page.evaluate(
    () =>
      new Promise<void>((resolve) => requestAnimationFrame(() => resolve())),
  );
  expect(calls).toHaveLength(1);
  await page.getByRole("button", { exact: true, name: "Fetch" }).click();
  await expect.poll(() => calls).toEqual(["cached", "no-cache"]);
});

test("malformed calendar error events cannot crash the view", async ({
  page,
}) => {
  const errors: string[] = [];

  page.on("pageerror", (error) => errors.push(error.message));
  await fixtures(page);
  await connect(page);
  await page.route("**/v1/calendar*", (route) =>
    route.fulfill({
      json: { events: [{ id: 1, kind: "episode", source: "s" }], sources: [] },
      status: 502,
    }),
  );
  await page.getByRole("link", { exact: true, name: "Calendar" }).click();
  await expect(page).toHaveURL((url) => url.pathname === "/calendar");
  await expect(page.locator("main [role=alert]")).toHaveText(
    "Request failed (502)",
  );
  expect(errors).toEqual([]);
});

test("iCalendar works through both image targets with a narrow calendar key", async ({
  request,
}) => {
  const calendarKey = "fake-calendar-key-0000000000000000000";

  for (const origin of ["http://127.0.0.1:4173", "http://127.0.0.1:4174"]) {
    const path = `${origin}/v1/calendar.ics?start=2026-10-01&end=2026-10-31`;
    const denied = await request.get(path);

    expect(denied.status()).toBe(401);

    const response = await request.get(path, {
      headers: { Authorization: `Bearer ${calendarKey}` },
    });
    const body = await response.text();

    expect(response.status()).toBe(200);
    expect(response.headers()["content-type"]).toContain("text/calendar");
    expect(body).toContain("BEGIN:VCALENDAR");
    expect(body.match(/\r\nPRODID:/g)).toHaveLength(1);
    expect(body).not.toContain("/data/media/");
    expect(body).not.toContain(calendarKey);
  }
});
