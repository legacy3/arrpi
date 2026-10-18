import { expect, type Page } from "@playwright/test";

import { mockConfigEditor, test } from "./fixtures/ui";

async function expectFocusClear(page: Page) {
  await expect
    .poll(() =>
      page.evaluate(() => {
        const target = document.activeElement;
        const header = document.querySelector("header");
        const actions = document.querySelector('[data-slot="form-actions"]');

        if (
          !(target instanceof HTMLElement) ||
          !target.closest("form") ||
          !header ||
          !actions
        ) {
          return { failure: "Expected focus in the settings form" };
        }

        const box = target.getBoundingClientRect();
        const top = header.getBoundingClientRect().bottom;
        const bottom = actions.contains(target)
          ? window.innerHeight
          : actions.getBoundingClientRect().top;

        return box.top >= top + 3 && box.bottom <= bottom - 3
          ? null
          : {
              bottom: box.bottom,
              control: target.id || target.textContent,
              top: box.top,
              visibleBottom: bottom,
              visibleTop: top,
            };
      }),
    )
    .toBeNull();
}

test.afterEach(async ({ page }) => {
  await page.unrouteAll({ behavior: "wait" });
});

for (const error of [false, true]) {
  test(`keyboard focus clears the sticky header and ${error ? "expanded error" : "normal"} actions`, async ({
    configDocument,
    connect,
    page,
  }) => {
    await mockConfigEditor(page, configDocument);

    if (error) {
      await page.route("**/v1/config", async (route) => {
        await (route.request().method() === "PUT"
          ? route.fulfill({
              json: {
                error:
                  "The cache budget must cover the response limit. Review both settings before saving again.",
              },
              status: 400,
            })
          : route.fallback());
      });
    }

    await connect("/settings/server");

    for (const title of ["Cache", "Requests", "HTTP listener"]) {
      await page.getByText(title, { exact: true }).click();
    }

    const first = page.getByLabel("Fresh data (seconds)", { exact: true });

    await expect(first).toBeVisible();

    if (error) {
      await page.getByRole("button", { exact: true, name: "Save" }).click();
      await expect(page.getByRole("main").getByRole("alert")).toContainText(
        "Review both settings",
      );
    }

    await expect
      .poll(() =>
        page.evaluate(() => {
          const actions = document.querySelector('[data-slot="form-actions"]');
          const height = document.documentElement.style.getPropertyValue(
            "--arrpi-form-actions-height",
          );

          return actions
            ? Number.parseFloat(height) ===
                Math.ceil(actions.getBoundingClientRect().height)
            : false;
        }),
      )
      .toBe(true);
    await first.focus();
    const count = await page.locator('form input:not([type="hidden"])').count();

    for (let index = 0; index <= count; index++) {
      await expectFocusClear(page);

      if (index < count) {
        await page.keyboard.press("Tab");
      }
    }

    for (let index = 0; index < count; index++) {
      await page.keyboard.press("Shift+Tab");
      await expectFocusClear(page);
    }

    await page.getByRole("link", { exact: true, name: "Calendar" }).click();
    await expect
      .poll(() =>
        page.evaluate(() =>
          document.documentElement.style.getPropertyValue(
            "--arrpi-form-actions-height",
          ),
        ),
      )
      .toBe("");
  });
}
