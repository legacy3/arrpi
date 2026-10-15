import { expect } from "@playwright/test";

import { mockConfigEditor, test } from "./fixtures/ui";

test.afterEach(async ({ page }) => {
  await page.unrouteAll({ behavior: "wait" });
});

test("connection rows report independent fresh health and retain lowercase source IDs", async ({
  configDocument,
  connect,
  page,
}, info) => {
  const { writes } = await mockConfigEditor(page, configDocument);
  const observed: string[] = [];
  const waiting = Promise.withResolvers<void>();

  await page.route("**/v1/config/connections/*/health", async (route) => {
    const request = route.request();
    const source = new URL(request.url()).pathname.split("/").at(-2) ?? "";

    observed.push(source);
    expect(request.method()).toBe("GET");
    expect(request.headers()["cache-control"]).toBe("no-cache");
    expect(request.postData()).toBeNull();

    if (source === "sonarr") {
      await waiting.promise;
    }

    await route.fulfill({
      json: {
        health_issue_count: null,
        source,
        status: {
          radarr: "unavailable",
          seerr: "authentication_failed",
          sonarr: "healthy",
        }[source],
      },
    });
  });

  try {
    await connect("/settings");
    await expect(
      page.getByLabel("sonarr: Configured · Checking…", { exact: true }),
    ).toBeVisible();
    await expect(
      page.getByLabel("radarr: Configured · Unavailable", { exact: true }),
    ).toBeVisible();
    await expect(
      page.getByLabel("seerr: Configured · Authentication failed", {
        exact: true,
      }),
    ).toBeVisible();
    await expect(
      page
        .getByRole("listitem")
        .filter({
          has: page.getByRole("button", { exact: true, name: "Edit Seerr" }),
        })
        .getByText("seerr", { exact: true }),
    ).toBeVisible();
  } finally {
    waiting.resolve();
  }

  await expect(
    page.getByLabel("sonarr: Configured · Healthy", { exact: true }),
  ).toBeVisible();
  expect(observed.toSorted((left, right) => left.localeCompare(right))).toEqual(
    [...Object.keys(configDocument.settings.sources), "seerr"].toSorted(
      (left, right) => left.localeCompare(right),
    ),
  );
  expect(writes).toHaveLength(0);
  await expect(
    page
      .getByRole("banner")
      .getByRole("link", { exact: true, includeHidden: true, name: "v0.1.1" }),
  ).toHaveAttribute("href", "https://github.com/legacy3/arrpi");
  await page.mouse.move(0, 0);
  await page.screenshot({
    fullPage: true,
    path: info.outputPath("connections.png"),
  });
});

test("unconfigured Seerr is not probed and HTTP failures are never displayed as healthy", async ({
  configDocument,
  connect,
  page,
}) => {
  configDocument.settings.seerr = null;
  const { writes } = await mockConfigEditor(page, configDocument);
  const observed: string[] = [];

  await page.route("**/v1/config/connections/*/health", async (route) => {
    const source =
      new URL(route.request().url()).pathname.split("/").at(-2) ?? "";

    observed.push(source);

    await (source === "sonarr"
      ? route.fulfill({
          json: { health_issue_count: 2, source, status: "attention" },
        })
      : route.fulfill({
          json: { error: "private native failure detail" },
          status: 503,
        }));
  });
  await connect("/settings");
  await expect(
    page.getByLabel("seerr: Not configured", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByLabel("sonarr: Configured · Needs attention", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByLabel("radarr: Configured · Check failed", { exact: true }),
  ).toBeVisible();
  expect(observed.toSorted((left, right) => left.localeCompare(right))).toEqual(
    Object.keys(configDocument.settings.sources).toSorted((left, right) =>
      left.localeCompare(right),
    ),
  );
  await expect(
    page.getByText("private native failure detail", { exact: true }),
  ).toHaveCount(0);
  expect(writes).toHaveLength(0);
});

test("API key hints react to the app without losing a credential draft", async ({
  configDocument,
  connect,
  page,
}) => {
  const { writes } = await mockConfigEditor(page, configDocument);

  await connect("/settings");
  await page.getByRole("button", { exact: true, name: "Add source" }).click();
  const key = page.getByLabel("API key", { exact: true });

  await expect(key).toHaveAttribute("placeholder", "Required");
  await key.fill("fake-credential-draft");
  await page.getByLabel("App", { exact: true }).selectOption("maintainerr");
  await expect(key).toHaveAttribute("placeholder", "Not required");

  for (const kind of ["sonarr", "radarr", "sabnzbd", "cleanuparr", "bazarr"]) {
    await page.getByLabel("App", { exact: true }).selectOption(kind);
    await expect(key).toHaveAttribute("placeholder", "Required");
    await expect(key).toHaveValue("fake-credential-draft");
  }

  await page.getByRole("button", { exact: true, name: "Back" }).click();
  await page.getByRole("button", { exact: true, name: "Edit sonarr" }).click();
  await expect(page.getByLabel("API key", { exact: true })).toHaveAttribute(
    "placeholder",
    "Unchanged",
  );
  await expect(page.getByLabel("API key", { exact: true })).toHaveValue("");
  await page.getByLabel("App", { exact: true }).selectOption("radarr");
  await expect(page.getByLabel("API key", { exact: true })).toHaveAttribute(
    "placeholder",
    "Unchanged",
  );
  await page.getByLabel("App", { exact: true }).selectOption("maintainerr");
  await expect(page.getByLabel("API key", { exact: true })).toHaveAttribute(
    "placeholder",
    "Not required",
  );
  expect(writes).toHaveLength(0);
});

test("real operator health works for native apps and Seerr without an actor binding", async ({
  connect,
  page,
  request,
}) => {
  const headers = {
    Authorization: "Bearer fake-browser-key-00000000000000000000",
  };

  for (const source of ["sonarr", "radarr", "seerr"]) {
    const path = `/v1/config/connections/${source}/health`;

    expect((await request.get(path)).status()).toBe(401);
    expect(
      (
        await request.get(path, {
          headers: {
            Authorization: "Bearer fake-calendar-key-0000000000000000000",
          },
        })
      ).status(),
    ).toBe(403);
    const response = await request.get(path, { headers });

    expect(response.ok()).toBe(true);
    expect(await response.json()).toEqual({
      health_issue_count: source === "seerr" ? null : 0,
      source,
      status: "healthy",
    });
  }

  await connect("/settings");

  for (const source of ["sonarr", "radarr", "seerr"]) {
    await expect(
      page.getByLabel(`${source}: Configured · Healthy`, { exact: true }),
    ).toBeVisible();
  }
});

test("editing an explicit German Seerr connection preserves its language", async ({
  configDocument,
  connect,
  page,
}) => {
  const seerr = configDocument.settings.seerr;

  if (!seerr) {
    throw new Error("Missing Seerr fixture");
  }

  seerr.language = "de";
  const { writes } = await mockConfigEditor(page, configDocument);

  await connect("/settings");
  await page.getByRole("button", { exact: true, name: "Edit Seerr" }).click();
  await expect(
    page.getByLabel("Catalog language", { exact: true }),
  ).toHaveValue("de");
  await page
    .getByLabel("Seerr URL", { exact: true })
    .fill("https://seerr.example.test/");
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText("Saved and applied.");
  expect(writes.at(-1)?.settings.seerr).toMatchObject({
    api_key: null,
    language: "de",
    url: "https://seerr.example.test/",
  });
});
