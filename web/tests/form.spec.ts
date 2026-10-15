import { expect, test } from "@playwright/test";

import { formInteger } from "../src/lib/form";
import { numericFields } from "../src/lib/generated/form-constraints";

test("account IDs cannot be rounded or encoded as missing numbers", () => {
  const form = new FormData();

  for (const value of ["", "0", "1.5", "NaN", "9007199254740993"]) {
    form.set("plex_id", value);
    expect(() => formInteger(form, "plex_id", { min: 1 })).toThrow();
  }

  form.set("plex_id", "101");
  expect(formInteger(form, "plex_id", { min: 1 })).toBe(101);
});

test("configuration parsing honors emitted Rust numeric bounds including valid zero values", () => {
  const form = new FormData();

  form.set("body", "67108865");
  expect(() =>
    formInteger(form, "body", numericFields.cache.max_body_bytes),
  ).toThrow();
  form.set("body", "1048576");
  expect(formInteger(form, "body", numericFields.cache.max_body_bytes)).toBe(
    1_048_576,
  );
  form.set("stale", "0");
  expect(formInteger(form, "stale", numericFields.cache.stale_seconds)).toBe(0);
  expect(() =>
    formInteger(form, "stale", numericFields.cache.fresh_seconds),
  ).toThrow();
  form.set("server_id", "0");
  expect(formInteger(form, "server_id")).toBe(0);
});
