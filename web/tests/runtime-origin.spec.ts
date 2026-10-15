import { expect, test } from "@playwright/test";

const origins = ["http://127.0.0.1:4173", "http://127.0.0.1:4176"];
const issuer = "fake-feed-issuer-key-0000000000000000000";

test("one UI artifact routes API and personal feeds to two independent runtime cores", async ({
  request,
}) => {
  const paths: string[] = [];

  for (const origin of origins) {
    expect((await request.get(`${origin}/ui-health`)).ok()).toBe(true);
    expect((await request.get(`${origin}/health`)).ok()).toBe(true);
    expect((await request.get(`${origin}/v1/status`)).status()).toBe(401);
    expect(
      (
        await request.get(`${origin}/v1/status`, {
          headers: { "X-ArrPi-Web-Auth": "proxy" },
        })
      ).status(),
    ).toBe(401);

    const issued = await request.post(`${origin}/v1/subscription`, {
      headers: {
        Authorization: `Bearer ${issuer}`,
        "X-ArrPi-Subject": crypto.randomUUID(),
      },
    });

    expect(issued.status()).toBe(200);
    const { rss_path: path } = (await issued.json()) as { rss_path: string };

    paths.push(path);
    const response = await request.get(
      `${origin}${path}?start=2026-10-01&end=2026-10-31`,
    );

    expect(response.status()).toBe(200);
    expect(response.headers()["content-type"]).toContain("application/rss+xml");
    expect(response.headers()["cache-control"]).toBe("private, no-store");
    expect(await response.text()).toContain("Moonberry Station");
  }

  for (const [index, origin] of origins.entries()) {
    expect((await request.get(origin + paths[1 - index])).status()).toBe(401);
  }
});
