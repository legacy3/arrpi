import { expect } from "@playwright/test";

import type { Status } from "../src/lib/api";

import { mockConfigEditor, test } from "./fixtures/ui";

test("section saves preserve masked credentials, mappings and unrelated settings", async ({
  configDocument: document,
  connect,
  page,
}) => {
  const original = structuredClone(document.settings);
  const { writes } = await mockConfigEditor(page, document);

  await connect("/settings");
  await expect(
    page.getByRole("heading", { exact: true, name: "Connections" }),
  ).toBeVisible();
  await expect(page.getByLabel("API key", { exact: true })).toHaveCount(0);
  await page.getByRole("link", { exact: true, name: "Feeds" }).click();
  await expect(page).toHaveURL(/\/settings\/feeds$/);
  await page.getByLabel("Title", { exact: true }).fill("Friends calendar");
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText("Saved and applied.");
  await expect(page.getByLabel("Title", { exact: true })).toHaveValue(
    "Friends calendar",
  );
  const saved = writes.at(-1)?.settings;

  expect(saved?.sources).toEqual(original.sources);
  expect(saved?.clients).toEqual(original.clients);
  expect(saved?.seerr).toEqual(original.seerr);
  expect(saved?.cache).toEqual(original.cache);
  expect(saved?.subscriptions).toEqual(original.subscriptions);
  expect(saved?.feed.title).toBe("Friends calendar");
  expect(saved?.sources.sonarr?.seerr_server_id).toBe(0);
  expect(saved?.clients.every((client) => client.key === null)).toBe(true);
});

test("viewer credentials cannot access any settings section", async ({
  page,
  request,
}) => {
  const key = "fake-calendar-key-0000000000000000000";

  expect(
    (
      await request.get("/v1/config", {
        headers: { Authorization: `Bearer ${key}` },
      })
    ).status(),
  ).toBe(403);
  await page.goto("/settings/feeds");
  await page.getByLabel("Client key", { exact: true }).fill(key);
  await page.getByRole("button", { exact: true, name: "Connect" }).click();
  await expect(
    page.getByText("Configuration access required", { exact: true }),
  ).toBeVisible();
  await expect(
    page
      .getByRole("navigation", { name: "Main navigation" })
      .getByRole("link", { exact: true, name: "Settings" }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("navigation", { name: "Settings sections" }),
  ).toHaveCount(0);
});

test("duplicate source names show an inline error without submitting credentials", async ({
  configDocument,
  connect,
  page,
}) => {
  const original = structuredClone(configDocument);
  const { writes } = await mockConfigEditor(page, configDocument);

  await connect("/settings");
  await page.getByRole("button", { exact: true, name: "Add source" }).click();
  await page.getByLabel("Name", { exact: true }).fill("sonarr");
  await page
    .getByLabel("URL", { exact: true })
    .fill("http://different.example");
  await page
    .getByLabel("API key", { exact: true })
    .fill("fake-replacement-key");
  await expect(page.getByRole("main").getByRole("alert")).toContainText(
    "A source with this name already exists",
  );
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toBeDisabled();
  expect(writes).toHaveLength(0);
  expect(configDocument).toEqual(original);
});

test("conflicts retain the draft until explicit reload and save the reviewed revision", async ({
  configDocument,
  connect,
  page,
}) => {
  const mock = await mockConfigEditor(page, configDocument);

  await connect("/settings/feeds");
  await page.getByLabel("Title", { exact: true }).fill("Unsaved draft");
  configDocument.revision += 1;
  configDocument.settings.feed.title = "Another editor";
  mock.setConflict(true);
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(
    page.getByRole("button", { exact: true, name: "Reload settings" }),
  ).toBeVisible();
  await expect(page.getByLabel("Title", { exact: true })).toHaveValue(
    "Unsaved draft",
  );
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toBeDisabled();
  await page
    .getByRole("button", { exact: true, name: "Reload settings" })
    .click();
  await expect(page.getByLabel("Title", { exact: true })).toHaveValue(
    "Another editor",
  );
  mock.setConflict(false);
  await page.getByLabel("Title", { exact: true }).fill("Reviewed change");
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText("Saved and applied.");
  expect(mock.writes).toHaveLength(2);
  expect(mock.writes.at(-1)?.revision).toBe(configDocument.revision - 1);
});

test("duplicate client names cannot reuse a stored account credential", async ({
  configDocument,
  connect,
  page,
}) => {
  const { writes } = await mockConfigEditor(page, configDocument);
  const name = configDocument.settings.clients[0]?.name ?? "";

  await connect("/settings/access");
  await page.getByRole("button", { exact: true, name: "Add client" }).click();
  await page.getByLabel("Name", { exact: true }).fill(name);
  await page
    .getByLabel("Client key", { exact: true })
    .fill("fake-new-account-key-00000000000000000");
  await page.getByLabel("Permissions", { exact: true }).fill("catalog:view");
  await expect(page.getByRole("main").getByRole("alert")).toContainText(
    "A client with this name already exists",
  );
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toBeDisabled();
  expect(writes).toHaveLength(0);
});

test("restart guidance survives the saved revision refresh", async ({
  configDocument,
  connect,
  page,
}) => {
  await mockConfigEditor(page, configDocument, true);
  await connect("/settings/server");
  await page.getByLabel("Maximum connections", { exact: true }).fill("65");
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText(
    "Saved. Restart ArrPi to apply connection or listener changes.",
  );
  await expect(
    page.getByLabel("Maximum connections", { exact: true }),
  ).toHaveValue("65");
});

test("adding one source refreshes discovery without reconnecting", async ({
  configDocument,
  connect,
  page,
}) => {
  await mockConfigEditor(page, configDocument);
  await page.route("**/v1/status", async (route) => {
    const response = await route.fetch();
    const status = (await response.json()) as Status;

    if (configDocument.settings.sources.extra) {
      status.sources.push({
        account: false,
        calendar: false,
        counters: { hits: 0, misses: 0, stale: 0, upstream_failures: 0 },
        endpoints: ["qualityprofile"],
        kind: "sonarr",
        name: "extra",
        summaries: [],
        tracking: false,
      });
    }

    await route.fulfill({ json: status });
  });
  await connect("/settings");
  await page.getByRole("button", { exact: true, name: "Add source" }).click();
  await page.getByLabel("Name", { exact: true }).fill("extra");
  await page.getByLabel("URL", { exact: true }).fill("http://extra.example");
  await page
    .getByLabel("API key", { exact: true })
    .fill("fake-extra-source-key");
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText("Saved and applied.");
  await expect(
    page.getByRole("button", { exact: true, name: "Edit extra" }),
  ).toBeVisible();
  await page.getByRole("link", { exact: true, name: "Explorer" }).click();
  await page.getByRole("combobox", { exact: true, name: "Source" }).click();
  await expect(
    page.getByRole("option", { exact: true, name: "extra" }),
  ).toBeVisible();
});

test("a new Seerr connection defaults to English and requires its own key", async ({
  configDocument,
  connect,
  page,
}) => {
  configDocument.settings.seerr = null;
  const { writes } = await mockConfigEditor(page, configDocument);

  await connect("/settings");
  await page.getByRole("button", { exact: true, name: "Edit Seerr" }).click();
  await expect(
    page.getByLabel("Catalog language", { exact: true }),
  ).toHaveValue("en");
  await page
    .getByLabel("Seerr URL", { exact: true })
    .fill("http://seerr.example");
  await page.getByLabel("Seerr API key", { exact: true }).focus();
  await page.getByLabel("Catalog language", { exact: true }).focus();
  await expect(page.getByRole("main").getByRole("alert")).toContainText(
    "Enter a Seerr API key",
  );
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toBeDisabled();
  expect(writes).toHaveLength(0);
  await page
    .getByLabel("Seerr API key", { exact: true })
    .fill("fake-new-seerr-key");
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText("Saved and applied.");
  expect(writes.at(-1)?.settings.seerr?.api_key).toBe("fake-new-seerr-key");
  expect(writes.at(-1)?.settings.seerr?.language).toBe("en");
});

test("editing a source retains its masked key and zero-valued profile mapping", async ({
  configDocument,
  connect,
  page,
}) => {
  const original = structuredClone(configDocument.settings);
  const { writes } = await mockConfigEditor(page, configDocument);

  await connect("/settings");
  await page.getByRole("button", { exact: true, name: "Edit sonarr" }).click();
  await expect(page.getByLabel("API key", { exact: true })).toHaveValue("");
  await page.getByLabel("URL", { exact: true }).fill("http://sonarr.example");
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText("Saved and applied.");
  expect(writes.at(-1)?.settings.sources.sonarr).toEqual({
    ...original.sources.sonarr,
    url: "http://sonarr.example",
  });
  expect(writes.at(-1)?.settings.clients).toEqual(original.clients);
});

test("referenced sources cannot be removed until their client grants are edited", async ({
  configDocument,
  connect,
  page,
}) => {
  const { writes } = await mockConfigEditor(page, configDocument);

  await connect("/settings");
  await page.getByRole("button", { exact: true, name: "Edit sonarr" }).click();
  await expect(
    page.getByRole("button", { exact: true, name: "Remove sonarr" }),
  ).toBeDisabled();
  await expect(
    page.getByText("Remove this source's permissions from Access first:", {
      exact: false,
    }),
  ).toBeVisible();
  expect(writes).toHaveLength(0);
});

test("removing an unreferenced source requires confirmation and supports conflict recovery", async ({
  configDocument,
  connect,
  page,
}) => {
  const source = configDocument.settings.sources.sonarr;

  if (!source) {
    throw new Error("Expected fixture source");
  }

  configDocument.settings.sources.unused = { ...source, seerr_server_id: null };
  const original = structuredClone(configDocument.settings);
  const mock = await mockConfigEditor(page, configDocument);

  await connect("/settings");
  await page.getByRole("button", { exact: true, name: "Edit unused" }).click();
  await page
    .getByRole("button", { exact: true, name: "Remove unused" })
    .click();
  expect(mock.writes).toHaveLength(0);
  mock.setConflict(true);
  configDocument.revision += 1;
  await page
    .getByRole("button", { exact: true, name: "Confirm removal" })
    .click();
  await expect(
    page.getByRole("button", { exact: true, name: "Confirm removal" }),
  ).toBeDisabled();
  await page
    .getByRole("button", { exact: true, name: "Reload settings" })
    .click();
  mock.setConflict(false);
  await page
    .getByRole("button", { exact: true, name: "Remove unused" })
    .click();
  await page
    .getByRole("button", { exact: true, name: "Confirm removal" })
    .click();
  await expect(
    page.getByRole("heading", { exact: true, name: "Connections" }),
  ).toBeVisible();
  expect(mock.writes.at(-1)?.settings.sources).toEqual(
    Object.fromEntries(
      Object.entries(original.sources).filter(([name]) => name !== "unused"),
    ),
  );
  expect(mock.writes.at(-1)?.settings.clients).toEqual(original.clients);
});

test("generated numeric bounds and the combined feed window block invalid saves", async ({
  configDocument,
  connect,
  page,
}) => {
  const { writes } = await mockConfigEditor(page, configDocument);

  await connect("/settings/feeds");
  await page.getByLabel("Past days", { exact: true }).fill("60");
  await page.getByLabel("Future days", { exact: true }).fill("60");
  await page.getByLabel("Title", { exact: true }).focus();
  await expect(page.getByRole("main").getByRole("alert")).toContainText(
    "Past and future days must total at most 92",
  );
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toBeDisabled();
  await page.getByRole("link", { exact: true, name: "Server" }).click();
  await page
    .getByLabel("Maximum response (bytes)", { exact: true })
    .fill("67108865");
  await page.getByLabel("Fresh data (seconds)", { exact: true }).focus();
  await expect(
    page.getByLabel("Maximum response (bytes)", { exact: true }),
  ).toHaveAttribute("aria-invalid", "true");
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toBeDisabled();
  expect(writes).toHaveLength(0);
});

test("settings sections support direct URLs and client-side history without signing in again", async ({
  configDocument,
  connect,
  page,
}) => {
  await mockConfigEditor(page, configDocument);
  await connect("/settings/feeds");
  await expect(
    page.getByRole("heading", { exact: true, name: "Calendar feeds" }),
  ).toBeVisible();
  const documents: string[] = [];

  page.on("request", (request) => {
    if (request.isNavigationRequest() && request.frame() === page.mainFrame()) {
      documents.push(request.url());
    }
  });
  await page.getByRole("link", { exact: true, name: "Access" }).click();
  await expect(page).toHaveURL(/\/settings\/access$/);
  await expect(
    page.getByRole("heading", { exact: true, name: "Client access" }),
  ).toBeVisible();
  await expect(
    page
      .getByRole("navigation", { name: "Main navigation" })
      .getByRole("link", { exact: true, name: "Settings" }),
  ).toHaveAttribute("aria-current", "page");
  await page.goBack();
  await expect(
    page.getByRole("heading", { exact: true, name: "Calendar feeds" }),
  ).toBeVisible();
  expect(documents).toEqual([]);
  await page.screenshot({
    fullPage: true,
    path: `test-results/settings-${test.info().project.name}.png`,
  });
});

test("account bindings require both IDs and reject overflowing Seerr IDs", async ({
  configDocument,
  connect,
  page,
}) => {
  const { writes } = await mockConfigEditor(page, configDocument);

  await connect("/settings/access");
  await page.getByRole("button", { exact: true, name: "Add client" }).click();
  await page.getByLabel("Name", { exact: true }).fill("new-account");
  await page
    .getByLabel("Client key", { exact: true })
    .fill("fake-new-account-key-00000000000000000");
  await page.getByLabel("Permissions", { exact: true }).fill("catalog:view");
  await page.getByLabel("Seerr user ID", { exact: true }).fill("1");
  await page.getByLabel("Plex user ID", { exact: true }).focus();
  await page.getByLabel("Name", { exact: true }).focus();
  await expect(page.getByRole("main").getByRole("alert")).toContainText(
    "Enter both Seerr and Plex user IDs",
  );
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toBeDisabled();
  await page.getByLabel("Plex user ID", { exact: true }).fill("999");
  await page.getByLabel("Seerr user ID", { exact: true }).fill("4294967296");
  await page.getByLabel("Name", { exact: true }).focus();
  await expect(
    page.getByLabel("Seerr user ID", { exact: true }),
  ).toHaveAttribute("aria-invalid", "true");
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toBeDisabled();
  expect(writes).toHaveLength(0);
});

test("editing one client preserves its masked token and Plex binding", async ({
  configDocument,
  connect,
  page,
}) => {
  const client = configDocument.settings.clients.find(
    (entry) => entry.seerr_user,
  );

  if (!client) {
    throw new Error("Expected a bound fixture client");
  }

  const original = structuredClone(configDocument.settings);
  const { writes } = await mockConfigEditor(page, configDocument);

  await connect("/settings/access");
  await page
    .getByRole("button", { exact: true, name: `Edit ${client.name}` })
    .click();
  await expect(page.getByLabel("Client key", { exact: true })).toHaveValue("");
  await expect(page.getByLabel("Name", { exact: true })).not.toBeEditable();
  await page
    .getByLabel("Permissions", { exact: true })
    .fill("catalog:view, requests:view, metrics");
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText("Saved and applied.");
  const saved = writes.at(-1)?.settings;

  expect(saved?.clients.find((entry) => entry.name === client.name)).toEqual({
    ...client,
    permissions: ["catalog:view", "requests:view", "metrics"],
  });
  expect(saved?.clients.filter((entry) => entry.name !== client.name)).toEqual(
    original.clients.filter((entry) => entry.name !== client.name),
  );
  expect(saved?.sources).toEqual(original.sources);
  expect(saved?.seerr).toEqual(original.seerr);
});

test("the shared configuration credential cannot be bound to a requester", async ({
  configDocument,
  connect,
  page,
}) => {
  const { writes } = await mockConfigEditor(page, configDocument);

  await connect("/settings/access");
  await page.getByRole("button", { exact: true, name: "Edit reader" }).click();
  await page.getByLabel("Seerr user ID", { exact: true }).fill("1");
  await page.getByLabel("Plex user ID", { exact: true }).fill("101");

  for (const permission of ["config:manage", "admin"]) {
    await page.getByLabel("Permissions", { exact: true }).fill(permission);
    await page.getByLabel("Seerr user ID", { exact: true }).focus();
    await expect(page.getByRole("main").getByRole("alert")).toContainText(
      "Keep configuration access and requester accounts on separate clients.",
    );
    await expect(
      page.getByRole("button", { exact: true, name: "Save" }),
    ).toBeDisabled();
  }

  expect(writes).toHaveLength(0);
});

for (const connection of [
  { label: "API key", name: "sonarr" },
  { label: "Seerr API key", name: "Seerr" },
]) {
  test(`${connection.name} trims native credentials only on submit and preserves blank stored keys`, async ({
    configDocument,
    connect,
    page,
  }) => {
    const { writes } = await mockConfigEditor(page, configDocument);

    await connect("/settings");
    await page
      .getByRole("button", { exact: true, name: `Edit ${connection.name}` })
      .click();
    await page.getByLabel(connection.label, { exact: true }).fill("   ");
    await page.getByRole("button", { exact: true, name: "Save" }).click();
    await expect(page.getByRole("status")).toHaveText("Saved and applied.");
    const saved = writes.at(-1)?.settings;

    expect(
      connection.name === "sonarr"
        ? saved?.sources.sonarr?.api_key
        : saved?.seerr?.api_key,
    ).toBeNull();
    await expect(
      page.getByRole("button", { exact: true, name: "Save" }),
    ).toBeFocused();
    await page
      .getByLabel(connection.label, { exact: true })
      .fill("  replacement-fake-key  ");
    await expect(page.getByRole("status")).toHaveCount(0);
    await page.getByRole("button", { exact: true, name: "Save" }).click();
    await expect(page.getByRole("status")).toHaveText("Saved and applied.");
    const updated = writes.at(-1)?.settings;

    expect(
      connection.name === "sonarr"
        ? updated?.sources.sonarr?.api_key
        : updated?.seerr?.api_key,
    ).toBe("replacement-fake-key");
    const action = page.getByRole("status").locator("..");

    await expect(
      action.getByRole("button", { exact: true, name: "Save" }),
    ).toBeVisible();
    await page.getByRole("button", { exact: true, name: "Back" }).click();
    await expect(page.getByRole("status")).toHaveCount(0);
  });
}

test("feed titles use the server byte limit and retain spaces while typing", async ({
  configDocument,
  connect,
  page,
}) => {
  configDocument.settings.feed.title = "Ä".repeat(128);
  const { writes } = await mockConfigEditor(page, configDocument);

  await connect("/settings/feeds");
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toBeEnabled();
  await page.getByLabel("Title", { exact: true }).fill("Ä".repeat(129));
  await page.getByLabel("Site URL", { exact: true }).focus();
  await expect(page.getByRole("main").getByRole("alert")).toContainText(
    "maximum 256 UTF-8 bytes",
  );
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toBeDisabled();
  await page.getByLabel("Title", { exact: true }).fill("");
  await page
    .getByLabel("Title", { exact: true })
    .pressSequentially("  Friends release calendar  ");
  await expect(page.getByLabel("Title", { exact: true })).toHaveValue(
    "  Friends release calendar  ",
  );
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText("Saved and applied.");
  expect(writes.at(-1)?.settings.feed.title).toBe("Friends release calendar");
});

test("editing tracking client access preserves its isolated native binding", async ({
  configDocument,
  connect,
  page,
}) => {
  const binding = {
    plex_id: 101,
    plex_server_owner: false,
    profile_id: "alice-profile",
    source: "crosswatch",
  };

  configDocument.settings.clients.push({
    key: null,
    name: "tracking-alice",
    permissions: ["crosswatch:tracking-view", "crosswatch:tracking-manage"],
    seerr_user: null,
    tracking_user: binding,
  });
  const { writes } = await mockConfigEditor(page, configDocument);

  await connect("/settings/access");
  await page
    .getByRole("button", { exact: true, name: "Edit tracking-alice" })
    .click();
  await page
    .getByLabel("Permissions", { exact: true })
    .fill("crosswatch:tracking-view");
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText("Saved and applied.");
  expect(
    writes
      .at(-1)
      ?.settings.clients.find((client) => client.name === "tracking-alice")
      ?.tracking_user,
  ).toEqual(binding);
});
