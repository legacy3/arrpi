import { z } from "zod";

import type { SettingsDocument } from "@/lib/settings";

import { isRequesterClient } from "@/lib/settings";

export function bindingSchema(settings: SettingsDocument, original?: string) {
  return z.object({
    client: z
      .string()
      .refine(
        (value) =>
          requesterClients(settings).some((client) => client.name === value),
        "Choose a Seerr-bound client without configuration access.",
      ),
    subject: z
      .string()
      .min(1, "Enter the provider subject.")
      .refine(
        (value) =>
          new TextEncoder().encode(value).length <= 256 &&
          !/\p{Cc}/u.test(value),
        "Use an exact subject of at most 256 UTF-8 bytes, without control characters.",
      )
      .refine(
        (value) =>
          !settings.oauth?.bindings.some(
            (binding) =>
              binding.subject === value && binding.subject !== original,
          ),
        "This provider subject is already bound.",
      ),
  });
}

export function oauthPrerequisite(settings: SettingsDocument) {
  if (!settings.mcp.enabled || !settings.mcp.public_url) {
    return "Enable MCP and set its public URL before enabling OAuth.";
  }

  if (!settings.seerr) {
    return "Connect Seerr before enabling OAuth.";
  }
}

export function providerSchema(settings: SettingsDocument) {
  return z.object({
    enabled: z
      .boolean()
      .refine((enabled) => !enabled || !oauthPrerequisite(settings), {
        message: "Complete the MCP and Seerr setup first.",
      }),
    issuer: z
      .string()
      .max(2048, "Keep the issuer within 2048 ASCII characters.")
      .refine((value) => {
        try {
          const url = new URL(value);
          const loopback = ["[::1]", "127.0.0.1", "localhost"].includes(
            url.hostname,
          );

          return (
            /^[\u0021-\u007E]+$/.test(value) &&
            (url.protocol === "https:" ||
              (url.protocol === "http:" && loopback)) &&
            value.startsWith(url.protocol + "//") &&
            Boolean(url.hostname) &&
            !url.username &&
            !url.password &&
            !value.includes("?") &&
            !value.includes("#")
          );
        } catch {
          return false;
        }
      }, "Use the provider’s exact HTTPS issuer URL, without credentials, query or fragment."),
    required_scope: z
      .string()
      .min(1, "Enter one required scope.")
      .max(128, "Keep the scope within 128 ASCII characters.")
      .regex(
        /^[\u0021\u0023-\u005B\u005D-\u007E]+$/,
        "Use one scope without spaces, quotes or backslashes.",
      ),
  });
}

export function requesterClients(settings: SettingsDocument) {
  return settings.clients.filter(isRequesterClient);
}
