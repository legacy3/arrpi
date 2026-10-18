import { expect } from "@playwright/test";

import { test } from "./fixtures/ui";

test("expanded settings show every field on a narrow screen", async ({
  connect,
  page,
}) => {
  await page.setViewportSize({ height: 740, width: 320 });
  await connect("/settings/mcp");
  await page
    .getByRole("button", { exact: true, name: "Advanced limits" })
    .click();
  const last = page.getByLabel("Queue wait (seconds)", { exact: true });

  await last.scrollIntoViewIfNeeded();
  await expect(last).toBeInViewport();
  const content = last.locator(
    'xpath=ancestor::*[@data-slot="accordion-content"]',
  );

  await expect
    .poll(() =>
      content.evaluate(
        (element) => element.scrollHeight - element.clientHeight,
      ),
    )
    .toBeLessThanOrEqual(1);
  await page.getByRole("button", { name: /^Tools \(/ }).click();
  const tools = page.getByRole("checkbox", { name: /^Enable / });

  await expect(tools.last()).toBeVisible();
  await tools.last().scrollIntoViewIfNeeded();
  await expect(tools.last()).toBeInViewport();
});

test("write outcomes stay readable beside a separate progress action", async ({
  connect,
  page,
}, info) => {
  const outcomes = [
    "invalid_arguments",
    "queued",
    "approved",
    "upstream_unauthorized",
    "already_available",
    "unknown_outcome",
  ];

  await page.route("**/v1/config/mcp/audit?*", (route) =>
    route.fulfill({
      json: {
        items: outcomes.map((outcome, index) => ({
          created_at: "2026-10-18T12:00:00Z",
          id: index + 1,
          outcome,
          target: {
            command_id: index === 1 ? 7765 : null,
            item_id: 7,
            source: "sonarr",
            title: `Moonberry Station ${index + 1}`,
          },
          tool: "refresh_media",
        })),
        next_offset: null,
        offset: 0,
        total: outcomes.length,
      },
    }),
  );
  await connect("/mcp");
  const history = page.getByRole("table", {
    exact: true,
    name: "Write history",
  });
  const statuses = history.locator('[data-slot="audit-outcome"]');

  await expect(statuses).toHaveCount(outcomes.length);
  await expect(history.locator('[data-slot="badge"]')).toHaveCount(0);
  const input = statuses.filter({ hasText: "Invalid input" });

  expect((await input.boundingBox())?.height).toBeLessThanOrEqual(22);
  const queued = history
    .getByRole("row")
    .filter({ hasText: "Moonberry Station 2" });
  const status = await queued
    .locator('[data-slot="audit-outcome"]')
    .boundingBox();
  const action = queued.getByRole("button", {
    exact: true,
    name: "Check progress for Moonberry Station 2",
  });
  const bounds = await action.boundingBox();

  expect(
    Math.abs(
      (status?.y ?? 0) +
        (status?.height ?? 0) / 2 -
        (bounds?.y ?? 0) -
        (bounds?.height ?? 0) / 2,
    ),
  ).toBeLessThanOrEqual(1);
  await action.focus();
  await expect(page.getByRole("tooltip")).toContainText(
    "Check command progress",
  );
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    fullPage: true,
    path: info.outputPath("history-outcomes.png"),
  });

  await page.setViewportSize({ height: 740, width: 320 });
  await expect(input).toBeVisible();
  expect((await input.boundingBox())?.height).toBeLessThanOrEqual(22);
  const title = queued.getByText("Moonberry Station 2", { exact: true });

  expect((await title.boundingBox())?.height).toBeLessThanOrEqual(44);
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
});

test("tool access retains readable columns beside long names and expandable detail", async ({
  connect,
  page,
}, info) => {
  await page.route("**/v1/mcp/status?*", (route) =>
    route.fulfill({
      json: {
        connected: true,
        tools: [
          {
            available: false,
            description:
              "Refresh metadata and scan files without searching for new releases.",
            name: "refresh_media",
            reason: "missing_grant",
            title: "Refresh library metadata and scan files",
          },
          ...[
            "account_not_linked",
            "account_permission",
            "account_unverified",
            "seerr_unavailable",
            "server_disabled",
            "tool_disabled",
            "unverified_version",
          ].map((reason) => ({
            available: false,
            description: "A fixture tool with a restricted access reason.",
            name: reason,
            reason,
            title: reason.replaceAll("_", " "),
          })),
        ],
        writes_supported: true,
      },
    }),
  );
  await connect("/mcp?view=tools");
  const table = page.getByRole("table", { exact: true, name: "Tools" });
  const row = table
    .getByRole("row")
    .filter({ hasText: "Refresh library metadata" });
  const access = row.getByRole("cell").nth(1);
  const badge = access.getByText("Needs grant", { exact: true });

  await expect(badge).toBeVisible();
  expect((await access.boundingBox())?.width).toBeGreaterThanOrEqual(120);
  expect((await badge.boundingBox())?.height).toBeLessThanOrEqual(26);

  for (const status of await table.locator('[data-slot="badge"]').all()) {
    expect((await status.boundingBox())?.height).toBeLessThanOrEqual(26);
  }

  await expect(row.getByText("refresh_media", { exact: true })).toBeHidden();
  await row
    .getByText("Refresh library metadata and scan files", { exact: true })
    .click();
  await expect(row.getByText("refresh_media", { exact: true })).toBeVisible();
  await expect(row).toContainText("without searching for new releases");
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    fullPage: true,
    path: info.outputPath("tool-columns.png"),
  });
});

test("Explorer selectors and refresh share a baseline without overflowing on phones", async ({
  connect,
  page,
}) => {
  await connect("/explorer?source=sonarr&operation=command%3Arefresh_media");
  const source = page.getByRole("combobox", { exact: true, name: "Source" });
  const operation = page.getByRole("combobox", {
    exact: true,
    name: "Operation",
  });
  const refresh = page.getByRole("button", {
    exact: true,
    name: "Refresh available operations",
  });

  await expect(operation).toContainText("Refresh metadata");
  const bounds = await Promise.all([
    source.boundingBox(),
    operation.boundingBox(),
    refresh.boundingBox(),
  ]);

  for (const bound of bounds) {
    expect(bound).not.toBeNull();
    expect(Math.abs((bound?.y ?? 0) - (bounds[0]?.y ?? 0))).toBeLessThanOrEqual(
      1,
    );
    expect(
      Math.abs((bound?.height ?? 0) - (bounds[0]?.height ?? 0)),
    ).toBeLessThanOrEqual(1);
  }

  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
});
