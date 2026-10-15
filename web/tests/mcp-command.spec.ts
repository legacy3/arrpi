import { expect, test } from "@playwright/test";
import { spawnSync } from "node:child_process";

import { mcpConnection } from "../src/lib/mcp-connection";

test("client commands preserve literal endpoints and tokens in POSIX shells", () => {
  const endpoint = "https://example.test/'$(printf INJECTED)/v1/mcp";
  const token = "fake-token-'$(printf INJECTED)-0000000000";
  const capture = String.raw`printf '%s\0' "$@"`;

  for (const kind of ["codex", "claude", "gemini"] as const) {
    const result = spawnSync(
      "/bin/sh",
      [
        "-c",
        `${kind}() { ${capture}; };\n${mcpConnection(kind, endpoint, token)}`,
      ],
      { encoding: "utf8" },
    );

    expect(result.status).toBe(0);
    expect(result.stderr).toBe("");
    const args = result.stdout.split("\0");

    expect(args.slice(0, 2)).toEqual(["mcp", "add"]);
    expect(args).toContain(endpoint);

    expect(args).toContain("Authorization: Bearer " + token);
    expect(mcpConnection(kind, endpoint, token)).not.toContain("\n");

    if (kind === "codex") {
      expect(args).toContain("mcp-remote@0.14.4");
      expect(args).not.toContain("--bearer-token-env-var");
    } else {
      expect(args).toContain("--scope");
      expect(args).toContain("user");
    }
  }
});
