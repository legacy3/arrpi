import { defineConfig, devices } from "@playwright/test";

export default defineConfig({
  projects: [
    {
      name: "desktop",
      use: {
        ...devices["Desktop Chrome"],
        viewport: { height: 850, width: 1280 },
      },
    },
    {
      name: "mobile",
      use: {
        ...devices["Desktop Chrome"],
        viewport: { height: 844, width: 390 },
      },
    },
  ],
  testDir: "./tests",
  timeout: 30_000,
  use: {
    baseURL: process.env.ARRPI_WEB_TEST_URL ?? "http://127.0.0.1:4173",
    screenshot: "only-on-failure",
  },
  workers: 4,
});
