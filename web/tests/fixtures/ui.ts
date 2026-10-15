import { test as base, expect, type Page } from "@playwright/test";

import type { SettingsEnvelope } from "../../src/lib/settings";

export const browserKey = "fake-browser-key-00000000000000000000";

export const test = base.extend<{
  configDocument: SettingsEnvelope;
  connect: (path?: string, key?: string) => Promise<void>;
}>({
  configDocument: async ({ request }, provide) => {
    const response = await request.get("/v1/config", {
      headers: { Authorization: `Bearer ${browserKey}` },
    });

    expect(response.status()).toBe(200);
    await provide((await response.json()) as SettingsEnvelope);
  },
  connect: async ({ page }, provide) => {
    await provide(async (path = "/", key = browserKey) => {
      await page.goto(path);
      await page.getByLabel("Client key", { exact: true }).fill(key);
      await page.getByRole("button", { exact: true, name: "Connect" }).click();
    });
  },
});

/** Each test owns its document and network interception, never shared mutable state. */
export async function mockConfigEditor(
  page: Page,
  document: SettingsEnvelope,
  restartRequired = false,
) {
  const writes: SettingsEnvelope[] = [];
  let conflict = false;

  await page.route("**/v1/config", async (route) => {
    if (route.request().method() === "PUT") {
      const saved = route.request().postDataJSON() as SettingsEnvelope;

      writes.push(saved);

      if (conflict) {
        await route.fulfill({
          json: { error: "configuration_changed" },
          status: 409,
        });
      } else {
        document.settings = saved.settings;
        document.revision += 1;
        await route.fulfill({
          json: {
            restart_required: restartRequired,
            revision: document.revision,
          },
        });
      }

      return;
    }

    await route.fulfill({ json: document });
  });

  return {
    setConflict: (value: boolean) => {
      conflict = value;
    },
    writes,
  };
}
