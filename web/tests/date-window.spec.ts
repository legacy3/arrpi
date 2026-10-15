import { expect } from "@playwright/test";

import { test } from "./fixtures/ui";

for (const scenario of [
  {
    end: "2028-03-31",
    instant: "2028-03-01T00:30:00Z",
    month: "2028-03",
    timezoneId: "America/Los_Angeles",
  },
  {
    end: "2028-02-29",
    instant: "2028-02-29T23:30:00Z",
    month: "2028-02",
    timezoneId: "Pacific/Kiritimati",
  },
]) {
  test.describe(`UTC month boundary in ${scenario.timezoneId}`, () => {
    test.use({ timezoneId: scenario.timezoneId });

    test("Calendar and Explorer use the same UTC month and inclusive date window", async ({
      connect,
      page,
    }) => {
      await page.clock.setFixedTime(new Date(scenario.instant));
      const calendarRead = page.waitForRequest(
        (request) => new URL(request.url()).pathname === "/v1/calendar",
      );

      await connect("/calendar");
      await expect(page.getByLabel("Month", { exact: true })).toHaveValue(
        scenario.month,
      );
      const calendarUrl = new URL((await calendarRead).url());

      expect(calendarUrl.searchParams.get("start")).toBe(
        `${scenario.month}-01`,
      );
      expect(calendarUrl.searchParams.get("end")).toBe(scenario.end);
      await page.getByLabel("Month", { exact: true }).fill("2030-05");
      await page.getByRole("button", { exact: true, name: "Today" }).click();
      await expect(page.getByLabel("Month", { exact: true })).toHaveValue(
        scenario.month,
      );
      await page.getByRole("link", { exact: true, name: "Explorer" }).click();
      await expect(
        page.getByRole("heading", { exact: true, name: "Explorer" }),
      ).toBeVisible();
      await expect(page.getByLabel("Month", { exact: true })).toHaveValue(
        scenario.month,
      );
      const nativeRead = page.waitForRequest((request) =>
        new URL(request.url()).pathname.endsWith("/api/v3/calendar"),
      );

      await page.getByRole("button", { exact: true, name: "Fetch" }).click();
      const nativeUrl = new URL((await nativeRead).url());

      expect(nativeUrl.searchParams.get("start")).toBe(`${scenario.month}-01`);
      expect(nativeUrl.searchParams.get("end")).toBe(scenario.end);
    });
  });
}
