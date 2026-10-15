import { expect } from "@playwright/test";

import type { Status } from "../src/lib/api";

import { browserKey, test } from "./fixtures/ui";

test.afterEach(async ({ page }) => {
  await page.unrouteAll({ behavior: "wait" });
});

test("proxy session remains mounted across every tab and browser history", async ({
  page,
}) => {
  await page.setExtraHTTPHeaders({ "X-ArrPi-Web-Auth": "proxy" });
  await page.route("**/v1/**", async (route) => {
    expect(route.request().headers().authorization).toBeUndefined();
    await route.continue({
      headers: {
        ...route.request().headers(),
        Authorization: `Bearer ${browserKey}`,
      },
    });
  });
  await page.goto("/");
  await expect(
    page.getByRole("heading", { exact: true, name: "Overview" }),
  ).toBeVisible();
  await page.waitForLoadState("networkidle");
  await page.locator("main").evaluate((element) => {
    element.dataset.navigationCheck = "same";
    document.documentElement.dataset.signinCheck = "0";
    const observer = new MutationObserver(() => {
      if (
        [...document.querySelectorAll('[role="status"]')].some(
          (node) => node.textContent?.trim() === "Signing in…",
        )
      ) {
        document.documentElement.dataset.signinCheck = "1";
      }
    });

    observer.observe(document.documentElement, {
      characterData: true,
      childList: true,
      subtree: true,
    });
  });
  const documents: string[] = [];
  const bootstraps: string[] = [];
  const components: { path: string; type: string }[] = [];

  page.on("request", (request) => {
    if (request.isNavigationRequest() && request.frame() === page.mainFrame()) {
      documents.push(new URL(request.url()).pathname);
    }

    if (new URL(request.url()).pathname === "/v1/status") {
      bootstraps.push(request.url());
    }
  });
  page.on("response", (response) => {
    if (response.request().headers().rsc === "1") {
      components.push({
        path: new URL(response.url()).pathname,
        type: response.headers()["content-type"] ?? "",
      });
    }
  });

  const settled = async (name: string, path: string) => {
    await expect(page).toHaveURL((url) => url.pathname === path);
    await expect(
      page.getByRole("heading", { exact: true, name }),
    ).toBeVisible();
    await page.waitForLoadState("networkidle");
    await expect(page).toHaveTitle(`${name} · ArrPi`);
    await expect(page.locator("main")).toHaveAttribute(
      "data-navigation-check",
      "same",
    );
    await expect(page.locator("html")).toHaveAttribute(
      "data-signin-check",
      "0",
    );
    await expect(
      page.getByRole("heading", { exact: true, name: "Connect" }),
    ).toHaveCount(0);
  };

  for (const [name, path] of [
    ["Calendar", "/calendar"],
    ["MCP", "/mcp"],
    ["Settings", "/settings"],
    ["Explorer", "/explorer"],
    ["Overview", "/"],
  ] as const) {
    await page.getByRole("link", { exact: true, name }).click();
    await settled(name, path);

    if (path === "/settings") {
      await page
        .getByRole("navigation", { name: "Settings sections" })
        .getByRole("link", { exact: true, name: "MCP" })
        .click();
      await settled("Settings", "/settings/mcp");
      await expect(
        page.getByRole("heading", { exact: true, name: "MCP server" }),
      ).toBeVisible();
    }
  }

  await page.goBack();
  await settled("Explorer", "/explorer");
  await page.goForward();
  await settled("Overview", "/");
  expect(documents).toEqual([]);
  expect(bootstraps).toEqual([]);
  expect(new Set(components.map((response) => response.path))).toEqual(
    new Set([
      "/",
      "/calendar",
      "/explorer",
      "/mcp",
      "/settings",
      "/settings/mcp",
    ]),
  );
  expect(
    components.every((response) =>
      response.type.startsWith("text/x-component"),
    ),
  ).toBe(true);
  expect(await page.content()).not.toContain(browserKey);
});

test("overview labels requester and future source kinds without inventing native API versions", async ({
  connect,
  page,
  request,
}) => {
  const response = await request.get("/v1/status", {
    headers: { Authorization: `Bearer ${browserKey}` },
  });

  expect(response.ok()).toBe(true);
  const status = (await response.json()) as Status;
  const sample = status.sources[0];

  if (!sample) {
    throw new Error("Expected a configured fake source");
  }

  status.sources = [
    ...status.sources.filter((source) => source.kind !== "seerr"),
    {
      ...sample,
      account: false,
      calendar: false,
      endpoints: [],
      kind: "seerr",
      name: "request-catalog",
    },
    {
      ...sample,
      account: false,
      calendar: false,
      endpoints: [],
      kind: "future-provider",
      name: "future-library",
    },
  ];
  await page.route("**/v1/status", (route) => route.fulfill({ json: status }));
  await connect();
  const requester = page
    .getByRole("row")
    .filter({ hasText: "request-catalog" });

  await expect(requester).toContainText("Seerr");
  await expect(requester).toContainText("API v1");
  await expect(requester).not.toContainText("Radarr");
  const future = page.getByRole("row").filter({ hasText: "future-library" });

  await expect(future).toContainText("future-provider");
  await expect(future).not.toContainText("API v3");
  await page.getByText("Runtime policy", { exact: true }).click();
  await expect(
    page.getByText("Retained body budget", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("Concurrent normalizations", { exact: true }),
  ).toBeVisible();
});
