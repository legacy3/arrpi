import { expect } from "@playwright/test";

import { mockConfigEditor, test } from "./fixtures/ui";

test("current connection health leads the overview on laptop and phone", async ({
  connect,
  page,
}, info) => {
  await page.route("**/v1/config/connections/*/health", (route) => {
    const source = new URL(route.request().url()).pathname.split("/").at(-2);

    return route.fulfill({
      json: {
        health_issue_count: source === "sonarr" ? 2 : 0,
        source,
        status: source === "sonarr" ? "attention" : "healthy",
      },
    });
  });
  await connect();
  const health = page.locator('[data-slot="card"]').filter({
    has: page.getByRole("heading", { exact: true, name: "Stack health" }),
  });

  await expect(health).toContainText("needs attention");
  await expect(health).toContainText("2 health issues");
  await expect(health).toBeInViewport();
  await expect(page.getByText("Cache hit rate", { exact: true })).toBeHidden();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    fullPage: true,
    path: info.outputPath("health-overview.png"),
  });
});

test("dirty forms prompt on navigation and keep credentials only in memory", async ({
  configDocument,
  connect,
  page,
}) => {
  await mockConfigEditor(page, configDocument);
  await connect("/settings/general");
  await page.getByLabel("Title", { exact: true }).fill("My unsaved stack");
  await page.getByRole("link", { exact: true, name: "Calendar" }).click();
  await expect(page.getByRole("alertdialog")).toContainText(
    "Discard unsaved changes?",
  );
  await page.getByRole("button", { exact: true, name: "Keep editing" }).click();
  await expect(page.getByLabel("Title", { exact: true })).toHaveValue(
    "My unsaved stack",
  );
  await page.getByRole("link", { exact: true, name: "Calendar" }).click();
  await page
    .getByRole("button", { exact: true, name: "Discard changes" })
    .click();
  await expect(page).toHaveURL(/\/calendar$/);
  expect(
    await page.evaluate(() => ({
      local: { ...localStorage },
      session: { ...sessionStorage },
    })),
  ).toEqual({ local: {}, session: {} });
});

test("fuzzy connection search and selected editor survive sidebar navigation", async ({
  connect,
  page,
}) => {
  await connect("/settings");
  await page.getByLabel("Search connections", { exact: true }).fill("sonar");
  await expect(
    page.getByRole("button", { exact: true, name: "Edit sonarr" }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { exact: true, name: "Edit radarr" }),
  ).toHaveCount(0);
  await page.getByRole("button", { exact: true, name: "Edit sonarr" }).click();
  await expect(page).toHaveURL(/entry=sonarr/);
  await page.getByRole("link", { exact: true, name: "Calendar" }).click();
  await page
    .getByRole("navigation", { name: "Main navigation" })
    .getByRole("link", { exact: true, name: "Settings" })
    .click();
  await expect(
    page.getByRole("heading", { exact: true, name: "Edit sonarr" }),
  ).toBeVisible();
  await page.getByRole("link", { exact: true, name: "Back" }).click();
  await expect(
    page.getByLabel("Search connections", { exact: true }),
  ).toHaveValue("sonar");
});

test("guided maintenance resolves titles and progress never reissues a write", async ({
  connect,
  page,
}) => {
  let writes = 0;

  await page.route("**/v1/tools/refresh_media", (route) => {
    writes += 1;
    expect(route.request().postDataJSON()).toEqual({
      item_id: 7,
      source: "sonarr",
    });

    return route.fulfill({
      json: {
        command_id: 7765,
        item_id: 7,
        source: "sonarr",
        status: "queued",
        title: "Moonberry Station",
      },
    });
  });
  await page.route("**/v1/tools/search_library", (route) =>
    route.fulfill({
      json: {
        items: [
          {
            id: 7,
            media_type: "tv",
            source: "sonarr",
            title: "Moonberry Station",
            year: 2026,
          },
        ],
        next_offset: null,
      },
    }),
  );
  await page.route("**/v1/tools/media_command_status", (route) =>
    route.fulfill({
      json: {
        command: "RefreshSeries",
        finished: true,
        status: "completed",
        successful: true,
      },
    }),
  );
  await connect("/explorer?source=sonarr&operation=command%3Arefresh_media");
  await page
    .getByLabel("Find installed media", { exact: true })
    .fill("Moonberry");
  await page
    .getByRole("button", { exact: true, name: "Moonberry Station (2026)" })
    .click();
  await expect(
    page.getByRole("button", { exact: true, name: "Run command" }),
  ).toBeDisabled();
  await page
    .getByRole("checkbox", {
      exact: true,
      name: "Apply this change to the library",
    })
    .check();
  await page.getByRole("button", { exact: true, name: "Run command" }).click();
  await page
    .getByRole("button", { exact: true, name: "Check command progress" })
    .click();
  await expect(page.getByRole("dialog")).toContainText("Completed");
  await page
    .getByRole("dialog")
    .getByRole("button", { exact: true, name: "Refresh" })
    .click();
  expect(writes).toBe(1);
  expect(page.url()).not.toContain("item_id");
});

test("calendar exposes availability and public details without shifting date-only releases", async ({
  connect,
  page,
}) => {
  const month = "2026-10";

  await page.route("**/v1/calendar?*", (route) =>
    route.fulfill({
      json: {
        events: [
          {
            date: "2026-10-16",
            has_file: true,
            id: 42,
            imdb_id: "tt1234567",
            kind: "digital",
            source: "radarr",
            title: "The Last Lantern",
            tmdb_id: 42,
          },
        ],
        sources: [
          { age_seconds: 0, cache: "hit", source: "radarr", status: "ok" },
        ],
      },
    }),
  );
  await connect(`/calendar?month=${month}`);
  await expect(page.getByText("Available", { exact: true })).toBeVisible();
  await page
    .getByRole("button", { exact: true, name: "The Last Lantern" })
    .click();
  const modal = page.getByRole("dialog");

  await expect(modal).toContainText("16 Oct");
  await expect(modal.getByRole("link", { name: "IMDb" })).toHaveAttribute(
    "href",
    "https://www.imdb.com/title/tt1234567",
  );
  await page.keyboard.press("Escape");
  await expect(modal).toHaveCount(0);
});

test("history uses complete-search filters, readable scope and effective tool access", async ({
  configDocument,
  connect,
  page,
}) => {
  await mockConfigEditor(page, configDocument);
  await page.route("**/v1/mcp/status?*", (route) =>
    route.fulfill({
      json: {
        connected: true,
        tools: [
          {
            available: false,
            description: "Refresh metadata and scan files.",
            name: "refresh_media",
            reason: "missing_grant",
            title: "Refresh metadata",
          },
        ],
        writes_supported: true,
      },
    }),
  );
  await page.route("**/v1/config/mcp/audit?*", (route) =>
    route.fulfill({
      json: {
        items: [
          {
            created_at: "2026-10-18T12:00:00Z",
            id: 1,
            outcome: "queued",
            target: {
              command_id: 7765,
              item_id: 7,
              media_type: "tv",
              missing_only: true,
              season: 3,
              source: "sonarr",
              title: "Moonberry Station",
              tmdb_id: 42,
            },
            tool: "refresh_media",
          },
        ],
        next_offset: null,
        offset: 0,
        total: 1,
      },
    }),
  );
  await connect("/mcp");
  const history = page.getByRole("table", {
    exact: true,
    name: "Write history",
  });

  await expect(history).toContainText("Moonberry Station");
  await expect(history).toContainText("S03, Missing only");
  await expect(history).toContainText("Accepted");
  await expect(
    history.getByRole("link", { exact: true, name: "Moonberry Station" }),
  ).toHaveAttribute("href", "https://www.themoviedb.org/tv/42");
  await history
    .getByRole("button", {
      exact: true,
      name: "Technical details for Moonberry Station",
    })
    .click();
  await expect(history.locator("code")).toContainText('"command_id":7765');
  await history
    .getByRole("button", {
      exact: true,
      name: "Technical details for Moonberry Station",
    })
    .click();
  await expect(history.locator("code")).toHaveCount(0);
  const filtered = page.waitForRequest(
    (request) =>
      new URL(request.url()).pathname === "/v1/config/mcp/audit" &&
      new URL(request.url()).searchParams.get("query") === "Moonbrry",
  );

  await page.getByLabel("Search history", { exact: true }).fill("Moonbrry");
  await filtered;
  await page.getByRole("tab", { exact: true, name: "Tools and usage" }).click();
  await page.getByLabel("Search tools", { exact: true }).fill("refesh");
  await expect(
    page.getByRole("table", { exact: true, name: "Tools" }),
  ).toContainText("Needs grant");
  await expect(
    page.getByRole("link", { name: /^Review access for / }),
  ).toHaveAttribute("href", /\/settings\/access\?entry=/);
});

test("guided series requests preserve typed separators and movie search has no season scope", async ({
  connect,
  page,
}) => {
  await connect(
    "/explorer?source=seerr&operation=command%3Arequest_series",
    "fake-requester-key-1111111111111111111",
  );
  await page.getByLabel("Seasons", { exact: true }).pressSequentially("1,2");
  await expect(page.getByLabel("Seasons", { exact: true })).toHaveValue("1,2");
  await page.getByRole("tab", { exact: true, name: "Advanced JSON" }).click();
  await expect(
    page.getByLabel("Arguments (JSON)", { exact: true }),
  ).toHaveValue(/"seasons": \[\s*1,\s*2\s*\]/);
  await page
    .getByRole("navigation", { name: "Main navigation" })
    .getByRole("link", { exact: true, name: "Overview" })
    .click();
  await expect(
    page.getByRole("heading", { exact: true, name: "Overview" }),
  ).toBeVisible();
  await page
    .getByRole("navigation", { name: "Main navigation" })
    .getByRole("link", { exact: true, name: "Explorer" })
    .click();
  await expect(page).toHaveURL(/source=seerr/);
});

test("permission picker contains its long list without extending the document", async ({
  connect,
  page,
}) => {
  await connect("/settings/access?entry=reader");
  const grants = page.getByRole("group", { exact: true, name: "Permissions" });

  await expect(grants).toBeVisible();
  const geometry = await page.evaluate(() => {
    const main = document.querySelector("main");
    const form = document.querySelector('[data-slot="form-actions"]');

    return {
      actions: form?.getBoundingClientRect().bottom ?? 0,
      document: document.documentElement.scrollHeight,
      main: main?.getBoundingClientRect().bottom ?? 0,
    };
  });

  expect(geometry.document - geometry.main).toBeLessThan(100);
});
