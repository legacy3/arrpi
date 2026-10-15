import { expect, test } from "@playwright/test";

const reader = "fake-browser-key-00000000000000000000";

for (const expiredStatus of [401, 302]) {
  test(`proxy sign-in survives reload and expires on ${expiredStatus} without browser credentials`, async ({
    page,
  }) => {
    await page.setExtraHTTPHeaders({ "X-ArrPi-Web-Auth": "proxy" });
    let expired = false;
    let statusReads = 0;
    const redirects: string[] = [];

    page.on("request", (request) => {
      if (new URL(request.url()).pathname === "/unexpected-login") {
        redirects.push(request.url());
      }
    });

    // Model the trusted reverse proxy; the browser itself sends no bearer key.
    await page.route("**/v1/**", async (route) => {
      expect(route.request().headers().authorization).toBeUndefined();

      if (new URL(route.request().url()).pathname === "/v1/status") {
        statusReads += 1;
      }

      if (expired) {
        await route.fulfill({
          headers:
            expiredStatus === 302 ? { Location: "/unexpected-login" } : {},
          json: { error: "authentication required" },
          status: expiredStatus,
        });

        return;
      }

      const response = await route.fetch({
        headers: {
          ...route.request().headers(),
          Authorization: `Bearer ${reader}`,
        },
      });

      await route.fulfill({ response });
    });
    await page.goto("/");
    await expect(
      page.getByRole("heading", { exact: true, name: "Overview" }),
    ).toBeVisible();
    await expect(page.getByLabel("Client key", { exact: true })).toHaveCount(0);
    await page.getByRole("link", { exact: true, name: "Settings" }).click();
    await expect(page).toHaveURL((url) => url.pathname === "/settings");
    await expect(
      page.getByRole("heading", { exact: true, name: "Connections" }),
    ).toBeVisible();
    expect(await page.content()).not.toContain(reader);
    expect(
      await page.evaluate(() => JSON.stringify([localStorage, sessionStorage])),
    ).not.toContain(reader);
    await page.reload();
    await expect(page).toHaveURL(/\/settings$/);
    await expect(
      page.getByRole("heading", { exact: true, name: "Connections" }),
    ).toBeVisible();
    await page.getByRole("link", { exact: true, name: "Overview" }).click();
    await expect(page).toHaveURL((url) => url.pathname === "/");
    expired = true;
    await page
      .getByRole("button", { exact: true, name: "Refresh cache status" })
      .click();
    await expect(
      page.getByRole("button", { exact: true, name: "Retry" }),
    ).toBeVisible();
    await expect(page.getByLabel("Client key", { exact: true })).toHaveCount(0);
    expect(redirects).toEqual([]);
    expect(statusReads).toBeLessThanOrEqual(4);
  });
}

test("presentation header alone cannot authorize native API reads", async ({
  page,
  request,
}) => {
  const headers = { "X-ArrPi-Web-Auth": "proxy" };

  expect((await request.get("/v1/status", { headers })).status()).toBe(401);
  await page.setExtraHTTPHeaders(headers);
  await page.goto("/");
  await expect(
    page.getByRole("button", { exact: true, name: "Retry" }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { exact: true, name: "Overview" }),
  ).toHaveCount(0);
});
