import type { Api, Status } from "@/lib/api";

export type Session = Readonly<{ api: Api; status: Status }>;

export async function openSession(api: Api): Promise<Session> {
  const { data: status } = await api.read<Status>("/v1/status");

  return { api, status };
}
