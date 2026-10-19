import { z } from "zod";

import { numericFields } from "@/lib/generated/form-constraints";
import { hasConfigAccess } from "@/lib/settings";

export const httpUrl = z
  .string()
  .trim()
  .pipe(z.url())
  .refine((value) => {
    if (!URL.canParse(value)) {
      return false;
    }

    const url = new URL(value);

    return (
      ["http:", "https:"].includes(url.protocol) &&
      !url.username &&
      !url.password &&
      !url.search &&
      !url.hash
    );
  }, "Use HTTP(S) without credentials, query or fragment");

const name = z
  .string()
  .regex(/^[a-z0-9-]{1,32}$/, "Use 1–32 lowercase letters, numbers or hyphens");
const key = z
  .string()
  .regex(/^[\u0021-\u007E]{32,128}$/, "Use 32–128 printable ASCII characters");
const optionalId = (min: number, max = Number.MAX_SAFE_INTEGER) =>
  z
    .string()
    .refine(
      (value) =>
        value === "" ||
        (/^\d+$/.test(value) &&
          Number.isSafeInteger(Number(value)) &&
          Number(value) >= min &&
          Number(value) <= max),
      `Enter a whole number from ${min} to ${max}`,
    );

export function clientSchema(
  names: string[],
  stored: boolean,
  oauthBound = false,
) {
  return z
    .object({
      key: stored ? z.union([z.literal(""), key]) : key,
      name: name.refine(
        (value) => !names.includes(value),
        "A client with this name already exists",
      ),
      permissions: z
        .string()
        .refine(
          (value) => value.split(",").some((part) => part.trim()),
          "Enter at least one permission",
        ),
      plex_id: optionalId(1),
      seerr_id: optionalId(1, 4_294_967_295),
    })
    .superRefine((value, context) => {
      if (oauthBound && (!value.seerr_id || !value.plex_id)) {
        context.addIssue({
          code: "custom",
          message: "Keep both user IDs while OAuth identities use this client.",
          path: [value.seerr_id ? "plex_id" : "seerr_id"],
        });
      }

      if (
        (oauthBound || Boolean(value.seerr_id) || Boolean(value.plex_id)) &&
        hasConfigAccess(
          value.permissions.split(",").map((permission) => permission.trim()),
        )
      ) {
        context.addIssue({
          code: "custom",
          message: oauthBound
            ? "Remove this client’s OAuth bindings before granting configuration access."
            : "Keep configuration access and requester accounts on separate clients.",
          path: ["permissions"],
        });
      }

      if (!oauthBound && Boolean(value.seerr_id) !== Boolean(value.plex_id)) {
        context.addIssue({
          code: "custom",
          message: "Enter both Seerr and Plex user IDs",
          path: [value.seerr_id ? "plex_id" : "seerr_id"],
        });
      }
    });
}

export function seerrSchema(stored: boolean) {
  return z.object({
    api_key: stored
      ? z.string().trim()
      : z.string().trim().min(1, "Enter a Seerr API key"),
    language: z
      .string()
      .trim()
      .regex(/^[a-zA-Z0-9-]{1,16}$/, "Use a language code, such as de or en"),
    url: httpUrl,
  });
}

export function sourceSchema(names: string[], stored: boolean) {
  return z
    .object({
      api_key: z.string().trim(),
      kind: z.enum([
        "plex",
        "sonarr",
        "radarr",
        "sabnzbd",
        "cleanuparr",
        "bazarr",
        "maintainerr",
        "crosswatch",
      ]),
      name: name
        .refine(
          (value) =>
            ![
              "calendar",
              "catalog",
              "config",
              "feeds",
              "issues",
              "requests",
              "seerr",
            ].includes(value),
          "This name is reserved",
        )
        .refine(
          (value) => !names.includes(value),
          "A source with this name already exists",
        ),
      seerr_server_id: optionalId(0, 4_294_967_295),
      url: httpUrl,
    })
    .superRefine((value, context) => {
      if (!stored && value.kind !== "maintainerr" && !value.api_key) {
        context.addIssue({
          code: "custom",
          message: "Enter an API key",
          path: ["api_key"],
        });
      }

      if (!["radarr", "sonarr"].includes(value.kind) && value.seerr_server_id) {
        context.addIssue({
          code: "custom",
          message: "Only Sonarr and Radarr support Seerr service bindings",
          path: ["seerr_server_id"],
        });
      }
    });
}

export const boundedNumber = ({ max, min }: { min: number; max: number }) =>
  z.number().int("Enter a whole number").min(min).max(max);
export const cacheSchema = z
  .object(
    Object.fromEntries(
      Object.entries(numericFields.cache).map(([name, bounds]) => [
        name,
        boundedNumber(bounds),
      ]),
    ) as Record<keyof typeof numericFields.cache, z.ZodNumber>,
  )
  .refine((value) => value.max_retained_bytes >= value.max_body_bytes, {
    message: "The shared cache budget must cover one maximum response",
    path: ["max_retained_bytes"],
  });
export const feedsSchema = z
  .object({
    enabled: z.boolean(),
    future_days: boundedNumber(numericFields.subscriptions.future_days),
    max_subjects: boundedNumber(numericFields.subscriptions.max_subjects),
    past_days: boundedNumber(numericFields.subscriptions.past_days),
  })
  .refine((value) => value.past_days + value.future_days <= 92, {
    message: "Past and future days must total at most 92",
    path: ["future_days"],
  });

export const instanceSchema = z.object({
  link: httpUrl,
  title: z
    .string()
    .trim()
    .min(1, "Enter a title")
    .refine(
      (value) => new TextEncoder().encode(value).length <= 256,
      "Use a shorter title (maximum 256 UTF-8 bytes)",
    )
    .refine(
      (value) => !/[\p{Cc}\uFFFE\uFFFF]/u.test(value),
      "Remove control characters from the title",
    ),
});
