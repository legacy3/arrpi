import { expect } from "@playwright/test";

import { choose, mockConfigEditor, setGrants, test } from "./fixtures/ui";

const issuer = "https://auth.example.test/application/o/arrpi/";

const binding = {
  client: "requester-1",
  issuer,
  subject: "stable-provider-subject",
};

test("disabling MCP also disables OAuth while preserving its identities", async ({
  configDocument: document,
  connect,
  page,
}) => {
  document.settings.mcp.public_url = "https://arrpi.example.test/v1/mcp";
  document.settings.oauth = {
    bindings: [binding],
    enabled: true,
    issuer,
    required_scope: "arrpi:mcp",
  };
  const { writes } = await mockConfigEditor(page, document);

  await connect("/settings/mcp");
  await page.getByLabel("Enable MCP", { exact: true }).uncheck();
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText("Saved and applied.");
  expect(writes.at(-1)?.settings.mcp.enabled).toBe(false);
  expect(writes.at(-1)?.settings.oauth).toEqual({
    bindings: [binding],
    enabled: false,
    issuer,
    required_scope: "arrpi:mcp",
  });
});

test("enabled OAuth explains the Seerr removal dependency", async ({
  configDocument: document,
  connect,
  page,
}) => {
  document.settings.mcp.public_url = "https://arrpi.example.test/v1/mcp";
  document.settings.oauth = {
    bindings: [binding],
    enabled: true,
    issuer,
    required_scope: "arrpi:mcp",
  };
  const { writes } = await mockConfigEditor(page, document);

  await connect("/settings");
  await page.getByRole("button", { exact: true, name: "Edit Seerr" }).click();
  await expect(
    page.getByText("Disable OAuth in OAuth settings before removing Seerr.", {
      exact: true,
    }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { exact: true, name: "Remove Seerr" }),
  ).toBeDisabled();
  expect(writes).toHaveLength(0);
});

test("identity capacity is visible and removing the policy preserves client keys", async ({
  configDocument: document,
  connect,
  page,
}) => {
  document.settings.oauth = {
    bindings: Array.from({ length: 64 }, (_, index) => ({
      ...binding,
      subject: `subject-${String(index)}`,
    })),
    enabled: false,
    issuer,
    required_scope: "arrpi:mcp",
  };
  const { writes } = await mockConfigEditor(page, document);

  await connect("/settings/oauth");
  await expect(
    page.getByRole("button", { exact: true, name: "Add identity" }),
  ).toBeDisabled();
  await expect(
    page.getByText(
      "64 identities configured. Remove an identity to add another.",
      { exact: true },
    ),
  ).toBeVisible();
  expect(writes).toHaveLength(0);
  const clients = structuredClone(document.settings.clients);

  await page
    .getByRole("button", { exact: true, name: "Remove OAuth configuration" })
    .click();
  await page
    .getByRole("button", { exact: true, name: "Confirm removal" })
    .click();
  await expect(page.getByLabel("Provider issuer")).toHaveValue("");
  expect(writes.at(-1)?.settings.oauth).toBeNull();
  expect(writes.at(-1)?.settings.clients).toEqual(clients);
});

test("provider and identity saves preserve credentials and bind only requester clients", async ({
  configDocument: document,
  connect,
  page,
}) => {
  document.settings.oauth = null;
  document.settings.mcp.public_url = "https://arrpi.example.test/v1/mcp";
  const original = structuredClone(document.settings);
  const { writes } = await mockConfigEditor(page, document);

  await connect("/settings/oauth");
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toBeDisabled();
  await page.getByLabel("Provider issuer").fill(issuer);
  await page.getByLabel("Enable OAuth").check();
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText("Saved and applied.");
  expect(writes.at(-1)?.settings.oauth).toEqual({
    bindings: [],
    enabled: true,
    issuer,
    required_scope: "arrpi:mcp",
  });
  expect(writes.at(-1)?.settings.clients).toEqual(original.clients);
  expect(writes.at(-1)?.settings.sources).toEqual(original.sources);
  expect(writes.at(-1)?.settings.seerr).toEqual(original.seerr);

  await page.getByRole("button", { exact: true, name: "Add identity" }).click();
  await page.getByLabel("ArrPi client").click();
  await expect(page.getByRole("option")).toHaveText([
    "requester-1",
    "requester-2",
  ]);
  await page.keyboard.press("Escape");
  await page.getByLabel("Provider subject").fill(binding.subject);
  await choose(page, "ArrPi client", binding.client);
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(
    page.getByRole("button", { exact: true, name: `Edit ${binding.subject}` }),
  ).toBeVisible();
  expect(writes.at(-1)?.settings.oauth?.bindings).toEqual([binding]);
  await expect(page.getByLabel("Provider issuer")).toHaveAttribute(
    "readonly",
    "",
  );
  await connect("/settings/oauth");
  await expect(
    page.getByRole("button", { exact: true, name: `Edit ${binding.subject}` }),
  ).toBeVisible();
});

test("duplicate subjects, UTF-8 byte limits and conflicting writes retain the reviewed identity", async ({
  configDocument: document,
  connect,
  page,
}) => {
  document.settings.oauth = {
    bindings: [binding],
    enabled: false,
    issuer,
    required_scope: "arrpi:mcp",
  };
  const mock = await mockConfigEditor(page, document);

  await connect("/settings/oauth");
  await page.getByRole("button", { exact: true, name: "Add identity" }).click();
  await page.getByLabel("Provider subject").fill(binding.subject);
  await page.getByLabel("Provider subject").blur();
  await expect(page.getByRole("main").getByRole("alert")).toContainText(
    "already bound",
  );
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toBeDisabled();
  await page.getByLabel("Provider subject").fill("ä".repeat(129));
  await expect(page.getByRole("main").getByRole("alert")).toContainText(
    "256 UTF-8 bytes",
  );
  expect(mock.writes).toHaveLength(0);

  await page.getByLabel("Provider subject").fill("second-exact-subject");
  await choose(page, "ArrPi client", "requester-2");
  mock.setConflict(true);
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(
    page.getByRole("button", { exact: true, name: "Reload settings" }),
  ).toBeVisible();
  await expect(page.getByLabel("Provider subject")).toHaveValue(
    "second-exact-subject",
  );
  await expect(page.getByLabel("ArrPi client")).toContainText("requester-2");
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toBeDisabled();
  expect(document.settings.oauth.bindings).toEqual([binding]);
});

test("OAuth dependencies block client removal until the identity is removed", async ({
  configDocument: document,
  connect,
  page,
}) => {
  document.settings.oauth = {
    bindings: [binding],
    enabled: false,
    issuer,
    required_scope: "arrpi:mcp",
  };
  const { writes } = await mockConfigEditor(page, document);

  await connect("/settings/access");
  await page
    .getByRole("button", { exact: true, name: "Edit requester-1" })
    .click();
  await expect(
    page.getByText(
      "Used by 1 OAuth identity. Remove these bindings in OAuth settings first.",
      { exact: true },
    ),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { exact: true, name: "Remove requester-1" }),
  ).toBeDisabled();
  await choose(page, "Requester account", "No requester account");
  await expect(page.getByRole("main").getByRole("alert")).toContainText(
    "Keep both user IDs",
  );
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toBeDisabled();
  await choose(page, "Requester account", "Fixture owner");
  await setGrants(page, ["Full administrator access"]);
  await expect(page.getByRole("main").getByRole("alert")).toContainText(
    "before granting configuration access",
  );
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toBeDisabled();
  expect(writes).toHaveLength(0);
  await page.getByRole("link", { exact: true, name: "OAuth" }).click();

  await page
    .getByRole("button", { exact: true, name: "Discard changes" })
    .click();

  await page
    .getByRole("button", { exact: true, name: `Edit ${binding.subject}` })
    .click();
  await page
    .getByRole("button", { exact: true, name: "Remove OAuth identity" })
    .click();
  await page
    .getByRole("button", { exact: true, name: "Confirm removal" })
    .click();
  await expect(
    page.getByRole("button", { exact: true, name: "Add identity" }),
  ).toBeVisible();
  expect(writes.at(-1)?.settings.oauth?.bindings).toEqual([]);
  await page.getByRole("link", { exact: true, name: "Access" }).click();
  await page
    .getByRole("button", { exact: true, name: "Edit requester-1" })
    .click();
  await expect(
    page.getByRole("button", { exact: true, name: "Remove requester-1" }),
  ).toBeEnabled();
});

test("editing an identity returns focus and feedback to its list", async ({
  configDocument: document,
  connect,
  page,
}) => {
  document.settings.oauth = {
    bindings: [binding],
    enabled: false,
    issuer,
    required_scope: "arrpi:mcp",
  };
  const { writes } = await mockConfigEditor(page, document);

  await connect("/settings/oauth");
  await expect(
    page.getByRole("heading", { exact: true, name: "OAuth identities" }),
  ).not.toBeFocused();
  await page
    .getByRole("button", { exact: true, name: `Edit ${binding.subject}` })
    .click();
  await expect(page.getByLabel("Provider subject")).toHaveAttribute(
    "readonly",
    "",
  );
  await choose(page, "ArrPi client", "requester-2");
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(
    page.getByRole("heading", { exact: true, name: "OAuth identities" }),
  ).toBeFocused();
  await expect(page.getByRole("status")).toHaveText("Saved and applied.");
  await expect(page.locator("form").getByRole("status")).toHaveCount(0);
  expect(writes.at(-1)?.settings.oauth?.bindings).toEqual([
    { ...binding, client: "requester-2" },
  ]);
});

test("removing the OAuth policy recovers after a revision conflict", async ({
  configDocument: document,
  connect,
  page,
}) => {
  document.settings.oauth = {
    bindings: [binding],
    enabled: false,
    issuer,
    required_scope: "arrpi:mcp",
  };
  const mock = await mockConfigEditor(page, document);

  await connect("/settings/oauth");
  await page
    .getByRole("button", { exact: true, name: "Remove OAuth configuration" })
    .click();
  document.revision += 1;
  mock.setConflict(true);
  await page
    .getByRole("button", { exact: true, name: "Confirm removal" })
    .click();
  await page
    .getByRole("button", { exact: true, name: "Reload settings" })
    .click();
  mock.setConflict(false);
  await expect(
    page.getByRole("button", {
      exact: true,
      name: "Remove OAuth configuration",
    }),
  ).toBeFocused();
  await page
    .getByRole("button", { exact: true, name: "Remove OAuth configuration" })
    .click();
  await expect(
    page.getByRole("button", { exact: true, name: "Confirm removal" }),
  ).toBeEnabled();
  await page
    .getByRole("button", { exact: true, name: "Confirm removal" })
    .click();
  await expect(page.getByLabel("Provider issuer")).toHaveValue("");
  await expect(page.getByRole("status")).toHaveText("Saved and applied.");
  await expect(page.getByRole("status")).toBeFocused();
  await expect(page.locator("form").getByRole("status")).toHaveCount(0);
  expect(mock.writes.at(-1)?.settings.oauth).toBeNull();
});

test("an externally removed identity stays closed if a later revision restores its subject", async ({
  configDocument: document,
  connect,
  page,
}) => {
  document.settings.oauth = {
    bindings: [binding],
    enabled: false,
    issuer,
    required_scope: "arrpi:mcp",
  };
  const mock = await mockConfigEditor(page, document);

  await connect("/settings/oauth");
  await page
    .getByRole("button", { exact: true, name: `Edit ${binding.subject}` })
    .click();
  await choose(page, "ArrPi client", "requester-2");
  document.settings.oauth.bindings = [];
  document.revision += 1;
  mock.setConflict(true);
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await page
    .getByRole("button", { exact: true, name: "Reload settings" })
    .click();
  await expect(
    page.getByRole("heading", { exact: true, name: "OAuth identities" }),
  ).toBeVisible();

  await page.getByLabel("Required scope").fill("arrpi:read");
  document.settings.oauth.bindings = [binding];
  document.revision += 1;
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await page
    .getByRole("button", { exact: true, name: "Reload settings" })
    .click();
  await expect(
    page.getByRole("button", { exact: true, name: `Edit ${binding.subject}` }),
  ).toBeVisible();
  await expect(page.getByLabel("Provider subject")).toHaveCount(0);
});

test("provider validation protects disabled drafts and explains missing MCP prerequisites", async ({
  configDocument: document,
  connect,
  page,
}) => {
  document.settings.oauth = null;
  document.settings.mcp.public_url = null;
  const { writes } = await mockConfigEditor(page, document);

  await connect("/settings/oauth");
  await expect(
    page.getByText("Enable MCP and set its public URL before enabling OAuth.", {
      exact: true,
    }),
  ).toBeVisible();

  for (const invalidIssuer of [
    "http://auth.example.test/",
    `${issuer}?`,
    `${issuer}#`,
  ]) {
    await page.getByLabel("Provider issuer").fill(invalidIssuer);
    await page.getByLabel("Provider issuer").blur();
    await expect(page.getByRole("main").getByRole("alert")).toContainText(
      "exact HTTPS issuer",
    );
    await expect(
      page.getByRole("button", { exact: true, name: "Save" }),
    ).toBeDisabled();
  }

  await page.getByLabel("Provider issuer").fill(issuer);
  await page.getByLabel("Required scope").fill("two scopes");
  await page.getByLabel("Required scope").blur();
  await expect(page.getByRole("main").getByRole("alert")).toContainText(
    "one scope",
  );
  await page.getByLabel("Required scope").fill("arrpi:mcp");
  await page.getByLabel("Enable OAuth").check();
  await page.getByLabel("Enable OAuth").blur();
  await expect(page.getByLabel("Enable OAuth")).toHaveAttribute(
    "aria-invalid",
    "true",
  );
  await expect(page.getByRole("main").getByRole("alert")).toContainText(
    "Complete the MCP and Seerr setup first.",
  );
  await expect(
    page.getByRole("button", { exact: true, name: "Save" }),
  ).toBeDisabled();
  expect(writes).toHaveLength(0);
  await page.getByLabel("Enable OAuth").uncheck();
  await page.getByRole("button", { exact: true, name: "Save" }).click();
  await expect(page.getByRole("status")).toHaveText("Saved and applied.");
  expect(writes.at(-1)?.settings.oauth?.enabled).toBe(false);
});
