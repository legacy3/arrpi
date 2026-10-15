import { expect, test } from "@playwright/test";

const reader = "fake-browser-key-00000000000000000000";
const calendarKey = "fake-calendar-key-0000000000000000000";

test("default API-only image exposes scoped data without a web shell", async ({
  request,
}) => {
  const base = "http://127.0.0.1:4174";

  expect((await request.get(`${base}/health`)).ok()).toBe(true);
  expect((await request.get(`${base}/`)).status()).toBe(404);
  expect((await request.get(`${base}/v1/status`)).status()).toBe(401);
  const headers = { Authorization: `Bearer ${calendarKey}` };
  const calendar = await request.get(
    `${base}/v1/calendar?start=2026-10-01&end=2026-10-31`,
    { headers },
  );

  expect(calendar.ok()).toBe(true);
  expect(await calendar.text()).toContain("Moonberry Station");
  expect(
    (
      await request.get(`${base}/v1/sonarr/api/v3/series`, { headers })
    ).status(),
  ).toBe(403);
});

test("Next proxies to the real Rust gateway and preserves scoped authorization", async ({
  request,
}) => {
  const shell = await request.get("/");
  const policy = shell.headers()["content-security-policy"];

  expect(shell.ok()).toBe(true);
  expect(policy).toContain("'nonce-");
  expect(policy).not.toContain("'unsafe-eval'");
  expect(
    policy?.split(";").find((rule) => rule.trim().startsWith("script-src")),
  ).not.toContain("'unsafe-inline'");
  const missing = await request.get("/missing-page");

  expect(missing.status()).toBe(404);
  expect(missing.headers()["content-security-policy"]).toContain("'nonce-");

  for (const path of ["/feeds", "/feeds/x", "/feeds/x/y/z"]) {
    const response = await request.get(path);

    expect(response.status()).toBe(404);
    expect(response.headers()["content-security-policy"]).toContain("'nonce-");
  }

  expect((await request.get("/v1/status")).status()).toBe(401);
  expect((await request.get("/v1/missing")).status()).toBe(401);
  expect((await request.get("/metrics")).status()).toBe(401);

  const headers = { Authorization: `Bearer ${calendarKey}` };
  const data = await request.get(
    "/v1/calendar?start=2026-10-01&end=2026-10-31",
    { headers },
  );

  expect(data.ok()).toBe(true);
  expect(data.headers()["cache-control"]).toBe("private, no-store");
  expect(await data.text()).not.toContain("/private/");
  expect(
    (await request.get("/v1/sonarr/api/v3/series", { headers })).status(),
  ).toBe(403);
  expect((await request.get("/metrics", { headers })).status()).toBe(403);
  expect(
    (
      await request.post("/v1/sonarr/api/v3/series", {
        headers: { Authorization: `Bearer ${reader}` },
      })
    ).status(),
  ).toBe(405);
});

test("browser connects through Next and renders the real Rust calendar", async ({
  page,
}, info) => {
  const errors: string[] = [];
  const violations: string[] = [];

  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (
      message.type() === "error" &&
      message.text().includes("Content Security Policy")
    ) {
      violations.push(message.text());
    }
  });
  await page.clock.install({ time: new Date("2026-10-13T12:00:00Z") });
  await page.goto("/");
  await page.getByLabel("Client key").fill(reader);
  await page.getByRole("button", { exact: true, name: "Connect" }).click();
  await expect(
    page.getByRole("heading", { exact: true, name: "Overview" }),
  ).toBeVisible();
  await page.getByRole("link", { exact: true, name: "Calendar" }).click();
  await expect(page).toHaveURL((url) => url.pathname === "/calendar");
  await expect(
    page.getByText("Moonberry Station", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("The Last Lantern", { exact: true }),
  ).toBeVisible();
  await page.getByRole("combobox", { name: "Calendar source" }).click();
  await page.getByRole("option", { exact: true, name: "All sources" }).click();
  await page.mouse.move(0, 0);
  expect(errors).toEqual([]);
  expect(violations).toEqual([]);
  await page.screenshot({
    fullPage: true,
    path: info.outputPath("calendar-integrated.png"),
  });
});

test("RSS works through both image targets with scoped credentials", async ({
  request,
}) => {
  for (const origin of ["http://127.0.0.1:4173", "http://127.0.0.1:4174"]) {
    const url = `${origin}/v1/calendar.rss?start=2026-10-01&end=2026-10-31`;

    expect((await request.get(url)).status()).toBe(401);
    const response = await request.get(url, {
      headers: { Authorization: `Bearer ${calendarKey}` },
    });
    const body = await response.text();

    expect(response.status()).toBe(200);
    expect(response.headers()["content-type"]).toContain("application/rss+xml");
    expect(response.headers()["cache-control"]).toBe("private, no-store");
    expect(body).toMatch(/<rss[^>]*version="2.0"/);
    expect(body).toContain('<guid isPermaLink="false">');
    expect(body).not.toContain("/private/");
    expect(body).not.toContain(calendarKey);
  }
});

test("personal feed URLs work without cookies, stay narrow and stop after rotation", async ({
  request,
}) => {
  const issuer = "fake-feed-issuer-key-0000000000000000000";

  for (const origin of ["http://127.0.0.1:4173", "http://127.0.0.1:4174"]) {
    const headers = {
      Authorization: `Bearer ${issuer}`,
      "X-ArrPi-Subject": crypto.randomUUID(),
    };
    const issued = await request.post(`${origin}/v1/subscription`, { headers });

    expect(issued.status()).toBe(200);
    const paths = (await issued.json()) as {
      rss_path: string;
      ical_path: string;
    };
    const token = paths.rss_path.split("/")[2];

    for (const path of [paths.rss_path, paths.ical_path]) {
      const response = await request.get(origin + path);

      expect(response.status()).toBe(200);
      expect(response.headers()["cache-control"]).toBe("private, no-store");
      expect(response.headers()["referrer-policy"]).toBe("no-referrer");
      expect(await response.text()).not.toContain(token ?? "invalid");
    }

    const denied = await request.get(
      `${origin}/v1/calendar?start=2026-10-01&end=2026-10-31`,
      { headers: { Authorization: `Bearer ${token}` } },
    );

    expect(denied.status()).toBe(401);
    expect(
      (
        await request.get(`${origin}${paths.rss_path}?apikey=${issuer}`)
      ).status(),
    ).toBe(400);
    const rotated = await request.post(`${origin}/v1/subscription/rotate`, {
      headers,
    });

    expect(rotated.status()).toBe(200);
    expect((await request.get(origin + paths.rss_path)).status()).toBe(401);
    const current = (await rotated.json()) as { rss_path: string };

    expect(
      (await request.delete(`${origin}/v1/subscription`, { headers })).status(),
    ).toBe(200);
    expect((await request.get(origin + current.rss_path)).status()).toBe(401);
  }
});
