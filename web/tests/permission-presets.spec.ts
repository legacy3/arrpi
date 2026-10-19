import { expect } from "@playwright/test";

import {
  matchingPermissionPreset,
  permissionOptions,
  permissionPresets,
} from "../src/lib/permissions";
import { choose, mockConfigEditor, setGrants, test } from "./fixtures/ui";

test("media assistant preset selects every media capability and preserves other clients", async ({
  configDocument,
  connect,
  page,
}, info) => {
  const client = configDocument.settings.clients.find(
    (entry) => entry.seerr_user,
  );

  if (!client) {
    throw new Error("Expected a requester fixture");
  }

  const original = structuredClone(configDocument.settings);
  const { writes } = await mockConfigEditor(page, configDocument);

  await connect(`/settings/access?entry=${client.name}`);
  await choose(page, "Permission preset", "Media assistant");
  await expect(
    page.getByText("All media permissions selected", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText(/Enable Advanced requests for this user in Seerr/),
  ).toBeVisible();
  expect(writes).toHaveLength(0);
  await page
    .getByRole("button", { exact: true, name: "Customize permissions" })
    .click();
  const native = page.getByRole("group", {
    exact: true,
    name: "sonarr: direct API reads",
  });

  await expect(native.getByRole("checkbox", { checked: true })).toHaveCount(0);
  await expect(
    page.getByRole("checkbox", { exact: true, name: "Manage ArrPi settings" }),
  ).not.toBeChecked();
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText("Saved and applied.");
  const saved = writes.at(-1)?.settings;
  const permissions =
    saved?.clients.find((entry) => entry.name === client.name)?.permissions ??
    [];

  expect(permissions).toEqual(
    permissionPresets(original.sources).find(
      (entry) => entry.id === "assistant",
    )?.grants,
  );
  expect(permissions).toEqual(
    expect.arrayContaining([
      "requests:create",
      "requests:cancel",
      "sonarr:library-refresh",
      "sonarr:library-search",
      "plex:ratings-view",
    ]),
  );
  expect(saved?.clients.filter((entry) => entry.name !== client.name)).toEqual(
    original.clients.filter((entry) => entry.name !== client.name),
  );
  expect(saved?.sources).toEqual(original.sources);
  expect(
    saved?.clients.find((entry) => entry.name === client.name)?.key,
  ).toBeNull();
  await page.screenshot({
    fullPage: true,
    path: info.outputPath("permission-presets.png"),
  });
});

test("read-only and API presets exclude writes and include only supported native reads", async ({
  configDocument,
  connect,
  page,
}) => {
  const client = configDocument.settings.clients.find(
    (entry) => entry.seerr_user,
  );

  if (!client) {
    throw new Error("Expected a requester fixture");
  }

  const { writes } = await mockConfigEditor(page, configDocument);

  await connect(`/settings/access?entry=${client.name}`);

  for (const label of ["Read only", "API reader"]) {
    await choose(page, "Permission preset", label);
    await page.getByRole("button", { exact: true, name: "Save" }).click();
    await expect(page.getByRole("status")).toHaveText("Saved and applied.");
    const permissions =
      writes
        .at(-1)
        ?.settings.clients.find((entry) => entry.name === client.name)
        ?.permissions ?? [];

    expect(permissions).not.toContain("requests:create");
    expect(permissions).not.toContain("issues:create");
    expect(permissions).not.toContain("sonarr:library-refresh");
    expect(permissions).not.toContain("sonarr:library-search");
    expect(permissions).not.toContain("admin");
    expect(permissions).not.toContain("config:manage");

    if (label === "API reader") {
      expect(permissions).toEqual(
        expect.arrayContaining(["sonarr:series", "radarr:movie", "metrics"]),
      );
      expect(permissions).not.toContain("sonarr:movie");
      expect(permissions).not.toContain("radarr:series");
    }
  }
});

test("bound requesters cannot choose the administrator preset", async ({
  configDocument,
  connect,
  page,
}) => {
  const client = configDocument.settings.clients.find(
    (entry) => entry.seerr_user,
  );

  if (!client) {
    throw new Error("Expected a requester fixture");
  }

  await mockConfigEditor(page, configDocument);
  await connect(`/settings/access?entry=${client.name}`);
  await page.getByRole("combobox", { name: "Permission preset" }).click();
  await expect(
    page.getByRole("option", { exact: true, name: "Administrator" }),
  ).toBeDisabled();
  await page.keyboard.press("Escape");
  await page
    .getByRole("button", { exact: true, name: "Customize permissions" })
    .click();
  await expect(
    page.getByRole("checkbox", {
      exact: true,
      name: "Full administrator access",
    }),
  ).toBeDisabled();
  await expect(
    page.getByRole("checkbox", { exact: true, name: "Manage ArrPi settings" }),
  ).toBeDisabled();
  await expect(
    page.getByText(/Create a separate client without linked accounts/),
  ).toBeVisible();
});

test("administrator displays inherited permissions as enabled and can return to custom access", async ({
  configDocument,
  connect,
  page,
}) => {
  await mockConfigEditor(page, configDocument);
  await connect("/settings/access?entry=reader");
  await choose(page, "Permission preset", "Administrator");
  await page
    .getByRole("button", { exact: true, name: "Customize permissions" })
    .click();
  const permissions = page.getByRole("group", {
    exact: true,
    name: "Permissions",
  });
  const browse = permissions.getByRole("checkbox", {
    exact: true,
    name: "Browse request catalog",
  });

  await expect(browse).toBeChecked();
  await expect(browse).toBeDisabled();
  await permissions
    .getByRole("checkbox", { exact: true, name: "Full administrator access" })
    .uncheck();
  await expect(browse).not.toBeChecked();
  await expect(browse).toBeEnabled();
  await expect(
    page.getByRole("combobox", { name: "Permission preset" }),
  ).toContainText("Custom permissions");
});

test("custom selection and fuzzy search remain usable on a narrow viewport", async ({
  configDocument,
  connect,
  page,
}, info) => {
  await mockConfigEditor(page, configDocument);
  await page.setViewportSize({ height: 740, width: 320 });
  await connect("/settings/access?entry=reader");
  await setGrants(page, ["Browse request catalog", "View own requests"]);
  await expect(
    page.getByRole("combobox", { name: "Permission preset" }),
  ).toContainText("Custom permissions");
  await page.getByLabel("Search permissions", { exact: true }).fill("nativ qu");
  await expect(
    page
      .getByRole("checkbox", { exact: true, name: "Read native queue" })
      .first(),
  ).toBeVisible();
  expect(
    await page.evaluate(
      () => document.documentElement.scrollWidth <= innerWidth,
    ),
  ).toBe(true);
  await page.screenshot({
    fullPage: true,
    path: info.outputPath("permission-presets-narrow.png"),
  });
});

test("preset definitions follow configured source names and keep tracking separate", () => {
  const sources = {
    scores: {
      api_key: null,
      kind: "plex",
      seerr_server_id: null,
      url: "https://fake-plex",
    },
    tracker: {
      api_key: null,
      kind: "crosswatch",
      seerr_server_id: null,
      url: "https://fake-crosswatch",
    },
    tv: {
      api_key: null,
      kind: "sonarr",
      seerr_server_id: 0,
      url: "https://fake-sonarr",
    },
  } as const;
  const assistant = permissionPresets(sources).find(
    (entry) => entry.id === "assistant",
  );

  expect(matchingPermissionPreset(["admin", "metrics"], sources)?.id).toBe(
    "admin",
  );
  expect(assistant?.grants).toContain("tv:library-search");
  expect(assistant?.grants).toContain("scores:ratings-view");
  expect(assistant?.grants).not.toContain("sonarr:library-search");
  expect(assistant?.grants).not.toContain("tracker:tracking-manage");
  expect(
    permissionOptions(sources).some(
      (option) => option.value === "tracker:tracking-manage",
    ),
  ).toBe(true);
});

test("last configuration client cannot lose its setup access through a preset", async ({
  configDocument,
  connect,
  page,
}) => {
  const { writes } = await mockConfigEditor(page, configDocument);

  await connect("/settings/access?entry=reader");
  await choose(page, "Permission preset", "Read only");
  await expect(page.getByRole("main").getByRole("alert")).toContainText(
    "Add another administrator before removing configuration access",
  );
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toBeDisabled();
  expect(writes).toHaveLength(0);
});

test("preset switches disclose removed permissions before saving", async ({
  configDocument,
  connect,
  page,
}) => {
  const client = configDocument.settings.clients.find(
    (entry) => entry.seerr_user,
  );

  if (!client) {
    throw new Error("Expected a requester fixture");
  }

  client.permissions.push("metrics");
  const { writes } = await mockConfigEditor(page, configDocument);

  await connect(`/settings/access?entry=${client.name}`);
  await choose(page, "Permission preset", "Media assistant");
  await expect(page.getByText(/On save, removes.*Read metrics/)).toBeVisible();
  expect(writes).toHaveLength(0);
  await page
    .getByRole("button", { exact: true, name: "Customize permissions" })
    .click();
  await page
    .getByRole("checkbox", { exact: true, name: "Request media" })
    .uncheck();
  await expect(
    page.getByText(/of \d+ media permissions selected/),
  ).toBeVisible();
  await expect(
    page.getByRole("combobox", { name: "Permission preset" }),
  ).toContainText("Custom permissions");
});
