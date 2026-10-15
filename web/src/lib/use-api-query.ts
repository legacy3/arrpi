"use client";

import {
  keepPreviousData,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";

import type { Api } from "@/lib/api";

export function useApiQuery<T>(
  api: Api,
  path: string,
  {
    enabled = true,
    initialData,
    initialFresh = false,
    keepPrevious = false,
  }: Readonly<{
    enabled?: boolean;
    initialFresh?: boolean;
    keepPrevious?: boolean;
    initialData?: T;
  }> = {},
) {
  const client = useQueryClient();
  const options = {
    enabled,
    initialData:
      initialData === undefined
        ? undefined
        : { age: null, cache: null, data: initialData, receivedAt: 0 },
    meta: { fresh: initialFresh },
    placeholderData: keepPrevious ? keepPreviousData : undefined,
    queryFn({
      meta,
      signal,
    }: {
      signal: AbortSignal;
      meta?: Record<string, unknown>;
    }) {
      return api.read<T>(path, meta?.fresh === true, signal);
    },
    queryKey: [path],
    staleTime: initialData === undefined ? 0 : Infinity,
  };
  const query = useQuery(options);

  function refresh(bypass = true) {
    void client
      .query({ ...options, meta: { fresh: bypass }, staleTime: 0 })
      .catch(() => {});
  }

  return {
    data: query.data,
    error: query.error,
    isError: query.isError,
    isFetching: query.isFetching,
    isPlaceholderData: query.isPlaceholderData,
    refresh,
  };
}
