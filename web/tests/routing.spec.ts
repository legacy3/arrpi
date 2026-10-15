import { expect, test } from "@playwright/test";

const key = "fake-browser-key-00000000000000000000";

test.afterEach(async ({ page }) => {
  await page.unrouteAll({ behavior: "wait" });
});

test("navigation changes URLs and preserves the session through browser history", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByLabel("Client key", { exact: true }).fill(key);
  await page.getByRole("button", { exact: true, name: "Connect" }).click();
  await expect(
    page.getByRole("heading", { exact: true, name: "Overview" }),
  ).toBeVisible();
  await page.locator("main").evaluate((element) => {
    element.dataset.routeCheck = "same";
  });
  await page.getByRole("link", { exact: true, name: "Calendar" }).click();
  await expect(page).toHaveURL((url) => url.pathname === "/calendar");
  await expect(page).toHaveURL(/\/calendar$/);
  await page.getByRole("link", { exact: true, name: "MCP" }).click();
  await expect(page).toHaveURL((url) => url.pathname === "/mcp");
  await expect(page).toHaveURL(/\/mcp$/);
  await expect(
    page.getByRole("heading", { exact: true, name: "Connect a client" }),
  ).toBeVisible();
  await page.goBack();
  await expect(page).toHaveURL(/\/calendar$/);
  await expect(
    page.getByRole("heading", { exact: true, name: "Calendar" }),
  ).toBeVisible();
  await page.goForward();
  await expect(page).toHaveURL(/\/mcp$/);
  await expect(
    page.getByRole("heading", { exact: true, name: "MCP" }),
  ).toBeVisible();
  expect(await page.locator("main").getAttribute("data-route-check")).toBe(
    "same",
  );
  await expect(page.getByLabel("Client key", { exact: true })).toHaveCount(0);
});

test("MCP deep links survive SSO reloads without client credentials in the browser", async ({
  page,
}) => {
  await page.setExtraHTTPHeaders({ "X-ArrPi-Web-Auth": "proxy" });
  await page.route("**/v1/**", async (route) => {
    expect(route.request().headers().authorization).toBeUndefined();
    await route.continue({
      headers: { ...route.request().headers(), Authorization: `Bearer ${key}` },
    });
  });
  await page.goto("/mcp");
  await expect(
    page.getByRole("heading", { exact: true, name: "Connect a client" }),
  ).toBeVisible();
  await page.waitForLoadState("networkidle");
  await page.reload();
  await expect(page).toHaveURL(/\/mcp$/);
  await expect(
    page.getByRole("heading", { exact: true, name: "Connect a client" }),
  ).toBeVisible();
  await expect(page.getByLabel("Client key", { exact: true })).toHaveCount(0);
  expect(await page.content()).not.toContain(key);
});

test("a viewer cannot load configuration through an admin deep link", async ({
  page,
}) => {
  const privateReads: string[] = [];

  page.on("request", (request) => {
    if (new URL(request.url()).pathname.startsWith("/v1/")) {
      privateReads.push(request.url());
    }
  });
  await page.goto("/mcp");
  await page
    .getByLabel("Client key", { exact: true })
    .fill("fake-calendar-key-0000000000000000000");
  await page.getByRole("button", { exact: true, name: "Connect" }).click();
  await expect(
    page.getByText("Configuration access required", { exact: true }),
  ).toBeVisible();
  await expect(
    page
      .getByRole("navigation", { name: "Main navigation" })
      .getByRole("link", { exact: true, name: "MCP" }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("heading", { exact: true, name: "Connect a client" }),
  ).toHaveCount(0);
  expect(privateReads.map((value) => new URL(value).pathname)).toEqual([
    "/v1/status",
  ]);
});

test("page render failures preserve the session and navigation", async ({
  page,
}) => {
  await page.route("**/v1/calendar?*", (route) =>
    route.fulfill({ json: { events: {}, sources: [] } }),
  );
  await page.goto("/");
  await page.getByLabel("Client key", { exact: true }).fill(key);
  await page.getByRole("button", { exact: true, name: "Connect" }).click();
  await expect(
    page.getByRole("heading", { exact: true, name: "Overview" }),
  ).toBeVisible();
  await page.locator("main").evaluate((element) => {
    element.dataset.routeCheck = "same";
  });
  await page.getByRole("link", { exact: true, name: "Calendar" }).click();
  await expect(page).toHaveURL((url) => url.pathname === "/calendar");
  await expect(
    page.getByText("Could not open this page", { exact: true }),
  ).toBeVisible();
  await page.getByRole("link", { exact: true, name: "Overview" }).click();
  await expect(
    page.getByRole("heading", { exact: true, name: "Overview" }),
  ).toBeVisible();
  await expect(page.getByLabel("Client key", { exact: true })).toHaveCount(0);
  expect(await page.locator("main").getAttribute("data-route-check")).toBe(
    "same",
  );
});
