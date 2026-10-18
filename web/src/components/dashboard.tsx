"use client";

import {
  QueryClient,
  QueryClientProvider,
  useIsFetching,
  useQueryClient,
} from "@tanstack/react-query";
import { usePathname } from "next/navigation";
import {
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import type { Api, Status } from "@/lib/api";
import type { Session } from "@/lib/session";

import { Connect, ProxyConnect } from "@/components/connect";
import { Notice } from "@/components/notice";
import { Shell, tabForPath } from "@/components/shell";
import { SessionContext } from "@/lib/session-context";
import { type Summary, summaryKinds } from "@/lib/summaries";
import { useApiQuery } from "@/lib/use-api-query";

export function Dashboard({
  children,
  proxyAuth = false,
}: Readonly<{ children: ReactNode; proxyAuth?: boolean }>) {
  const [session, setSession] = useState<Session | null>(null);
  const [error, setError] = useState("");
  const activeRef = useRef<Api | null>(null);
  const disconnect = useCallback(() => {
    activeRef.current?.close();
    activeRef.current = null;
    setSession(null);
    setError("");
  }, []);

  useEffect(() => () => activeRef.current?.close(), []);

  const connected = useCallback((value: Session) => {
    activeRef.current = value.api;
    setSession(value);
  }, []);

  const expired = useCallback(() => {
    disconnect();

    if (!proxyAuth) {
      setError("authentication required");
    }
  }, [disconnect, proxyAuth]);

  return session ? (
    <Connected
      session={session}
      disconnect={proxyAuth ? undefined : disconnect}
    >
      {children}
    </Connected>
  ) : (
    <Shell tab="overview" connected={false}>
      {proxyAuth ? (
        <ProxyConnect connected={connected} expired={expired} />
      ) : (
        <Connect error={error} connected={connected} expired={expired} />
      )}
    </Shell>
  );
}

function Connected({
  children,
  disconnect,
  session,
}: Readonly<{
  children: ReactNode;
  session: Session;
  disconnect?: () => void;
}>) {
  const [client] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: {
            gcTime: 0,
            refetchOnReconnect: false,
            refetchOnWindowFocus: false,
            retry: false,
          },
        },
      }),
  );

  useEffect(() => () => client.clear(), [client]);

  return (
    <QueryClientProvider client={client}>
      <SessionView session={session} disconnect={disconnect}>
        {children}
      </SessionView>
    </QueryClientProvider>
  );
}

function SessionView({
  children,
  disconnect,
  session,
}: Readonly<{
  children: ReactNode;
  session: Session;
  disconnect?: () => void;
}>) {
  const tab = tabForPath(usePathname());
  const query = useApiQuery<Status>(session.api, "/v1/status", {
    initialData: session.status,
  });
  const status = query.data?.data ?? session.status;
  const client = useQueryClient();
  const summariesFetching = useIsFetching({
    predicate: (item) =>
      typeof item.queryKey[0] === "string" &&
      item.queryKey[0].startsWith("/v1/summaries/"),
  });
  const value = useMemo(
    () => ({ api: session.api, status }),
    [session.api, status],
  );

  function refreshOverview() {
    query.refresh(false);
    void client.invalidateQueries({ queryKey: ["connection-health"] });

    for (const kind of summaryKinds(status.sources)) {
      const path = `/v1/summaries/${kind}`;

      void client
        .query({
          queryFn: ({ signal }) =>
            session.api.read<Summary>(path, true, signal),
          queryKey: [path],
          staleTime: 0,
        })
        .catch(() => {});
    }
  }

  return (
    <Shell
      tab={tab}
      connected
      version={status.version}
      instance={status.instance.title}
      configManage={status.config_manage}
      disconnect={disconnect}
      refresh={tab === "overview" ? refreshOverview : undefined}
      refreshing={query.isFetching || summariesFetching > 0}
    >
      {query.error && <Notice>{query.error.message}</Notice>}
      <SessionContext value={value}>{children}</SessionContext>
    </Shell>
  );
}
