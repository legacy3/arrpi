import { expect } from "@playwright/test";

import { mockConfigEditor, test } from "./fixtures/ui";

test.afterEach(async ({ page }) => {
  await page.unrouteAll({ behavior: "wait" });
});

test("large Explorer responses scroll inside the output instead of growing the page", async ({
  connect,
  page,
}) => {
  await page.route("**/api/v3/series?*", (route) =>
    route.fulfill({
      json: Array.from({ length: 500 }, (_, id) => ({
        id,
        title: `Series ${id}`,
      })),
    }),
  );
  await connect("/explorer");
  await expect(
    page.getByRole("heading", { exact: true, name: "Explorer" }),
  ).toBeVisible();
  await page.getByRole("combobox", { exact: true, name: "Source" }).click();
  await page.getByRole("option", { exact: true, name: "sonarr" }).click();
  await expect(
    page.getByRole("combobox", { exact: true, name: "Source" }),
  ).toBeFocused();
  await page.getByRole("combobox", { exact: true, name: "Operation" }).click();
  await page.getByRole("option", { exact: true, name: "series" }).click();
  await page.getByRole("button", { exact: true, name: "Fetch" }).click();
  const output = page.getByLabel("Native JSON response", { exact: true });

  await expect(output).toHaveValue(/Series 499/);
  const size = await output.evaluate((node) => ({
    height: node.clientHeight,
    scroll: node.scrollHeight,
    viewport: window.innerHeight,
  }));

  expect(size.height).toBeLessThanOrEqual(size.viewport * 0.7 + 1);
  expect(size.scroll).toBeGreaterThan(size.height);
  await page.setViewportSize({ height: 390, width: 844 });
  await expect
    .poll(() => output.evaluate((node) => node.clientHeight))
    .toBeLessThanOrEqual(390 * 0.7 + 1);
});

test("server validation reasons stay next to the shared save action", async ({
  configDocument,
  connect,
  page,
}) => {
  await mockConfigEditor(page, configDocument);
  await page.route("**/v1/config", async (route) => {
    await (route.request().method() === "PUT"
      ? route.fulfill({
          json: {
            code: "invalid_configuration",
            error: "Feed link must not contain a query",
          },
          status: 400,
        })
      : route.fallback());
  });
  await connect("/settings/feeds");
  await page
    .getByLabel("Site URL", { exact: true })
    .fill("https://example.invalid/");
  const save = page.getByRole("button", { exact: true, name: "Save" });

  await save.click();
  await expect(page.getByRole("main").getByRole("alert")).toContainText(
    "Feed link must not contain a query",
  );
  await expect(save).toBeEnabled();
  await expect(save).toBeInViewport();
});

test("setup hints do not announce themselves as errors", async ({
  configDocument,
  connect,
  page,
}) => {
  configDocument.settings.seerr = null;
  await mockConfigEditor(page, configDocument);
  await connect("/mcp");
  await expect(
    page.getByText("to enable MCP.", { exact: false }),
  ).toBeVisible();
  await expect(page.getByRole("main").getByRole("alert")).toHaveCount(0);
});

test("entry editors focus their headings when opened", async ({
  configDocument,
  connect,
  page,
}) => {
  await mockConfigEditor(page, configDocument);
  await connect("/settings");
  await page.getByRole("button", { exact: true, name: "Add source" }).click();
  await expect(
    page.getByRole("heading", { exact: true, name: "Add source" }),
  ).toBeFocused();
  await expect(page.getByLabel("URL", { exact: true })).toBeVisible();
  await page.getByRole("button", { exact: true, name: "Back" }).click();
  await expect(
    page.getByRole("heading", { exact: true, name: "Connections" }),
  ).toBeFocused();
  await page.getByRole("link", { exact: true, name: "Access" }).click();
  await page.getByRole("button", { exact: true, name: "Add client" }).click();
  await expect(
    page.getByRole("heading", { exact: true, name: "Add client" }),
  ).toBeFocused();
  await expect(page.getByLabel("Client key", { exact: true })).toBeVisible();
  await expect(
    page.getByRole("heading", { exact: true, name: "Settings" }),
  ).toBeVisible();
});

test("successful saves show their outcome beside the pinned action", async ({
  configDocument,
  connect,
  page,
}) => {
  await mockConfigEditor(page, configDocument);
  await connect("/settings/feeds");
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  const outcome = page
    .getByRole("status")
    .filter({ hasText: "Saved and applied." });

  await expect(outcome).toBeVisible();
  await expect(outcome).toBeInViewport();
});
