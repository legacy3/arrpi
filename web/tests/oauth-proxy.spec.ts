import { expect, test } from "@playwright/test";

test("optional UI preserves finite OAuth metadata routes and MCP challenges", async ({
  request,
}) => {
  for (const [ui, core] of [
    ["http://127.0.0.1:4173", "http://127.0.0.1:4174"],
    ["http://127.0.0.1:4176", "http://127.0.0.1:4175"],
  ]) {
    for (const path of [
      "/.well-known/oauth-protected-resource",
      "/.well-known/oauth-protected-resource/v1/mcp",
    ]) {
      const direct = await request.get(core + path);
      const proxied = await request.get(ui + path);

      expect(proxied.status()).toBe(direct.status());
      expect(proxied.headers()["content-type"]).toBe(
        direct.headers()["content-type"],
      );
      expect(await proxied.text()).toBe(await direct.text());
      expect(proxied.headers()["content-type"] ?? "").not.toContain(
        "text/html",
      );
    }

    const direct = await request.post(`${core}/v1/mcp`, { data: {} });
    const proxied = await request.post(`${ui}/v1/mcp`, { data: {} });

    expect(proxied.status()).toBe(401);
    expect(proxied.headers()["www-authenticate"]).toBe(
      direct.headers()["www-authenticate"],
    );
    expect(await proxied.json()).toEqual(await direct.json());
    const unknown = await request.get(`${ui}/.well-known/private-settings`);

    expect(unknown.status()).toBe(404);
    expect(unknown.headers()["content-type"]).toContain("text/html");
  }
});
