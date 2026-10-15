import { expect, test } from "@playwright/test";
import { Ajv2020 } from "ajv/dist/2020.js";

import type { Summary } from "../src/lib/summaries";

import schema from "../../docs/api/public.schema.json" with { type: "json" };
import { browserKey } from "./fixtures/ui";

const validator = new Ajv2020({ strict: false, validateFormats: false });
const validate = validator.compile<Summary>({
  $ref: "#/components/schemas/Summary",
  components: schema.components,
});

for (const origin of ["http://127.0.0.1:4173", "http://127.0.0.1:4174"]) {
  for (const kind of ["library", "queue", "status"]) {
    test(`${kind} adapter responses match generated Rust contracts through ${origin}`, async ({
      request,
    }) => {
      const response = await request.get(
        `${origin}/v1/summaries/${kind}?no_cache=true`,
        { headers: { Authorization: `Bearer ${browserKey}` } },
      );
      const body: unknown = await response.json();

      expect(response.ok()).toBe(true);

      if (!validate(body)) {
        throw new Error(
          `Public summary contract mismatch: ${JSON.stringify(validate.errors)}`,
        );
      }

      expect(body.complete).toBe(true);
      expect(body.sources).toHaveLength(kind === "status" ? 4 : 2);
      expect(body.sources.every((row) => row.status === "ok")).toBe(true);
      expect(JSON.stringify(body)).not.toContain("private");
    });
  }
}

test("the public contract rejects mistyped counters and missing summary payloads", () => {
  const freshness = {
    bypass_requested: false,
    cache: "hit",
    checked_at: null,
    source: "movies",
  };

  for (const data of [{ complete: true, items: "four" }, { complete: true }]) {
    expect(
      validate({
        complete: true,
        sources: [
          { data, freshness, kind: "radarr", source: "movies", status: "ok" },
        ],
      }),
    ).toBe(false);
  }
});
