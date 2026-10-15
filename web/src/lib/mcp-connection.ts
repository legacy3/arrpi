export const mcpClients = [
  { id: "claude", label: "Claude Code" },
  { id: "codex", label: "Codex" },
  { id: "gemini", label: "Gemini CLI" },
] as const;

export type ConnectionKind = (typeof mcpClients)[number]["id"];

export function mcpConnection(
  kind: ConnectionKind,
  endpoint: string,
  token: string,
) {
  const header = shellQuote("Authorization: Bearer " + token);
  const url = shellQuote(endpoint);

  return kind === "codex"
    ? `codex mcp add arrpi -- npx --yes mcp-remote@0.14.4 ${url} --transport http-only --header ${header}`
    : `${kind} mcp add --transport http --scope user arrpi ${url} --header ${header}`;
}

function shellQuote(value: string) {
  return `'${value.replaceAll("'", "'\"'\"'")}'`;
}
