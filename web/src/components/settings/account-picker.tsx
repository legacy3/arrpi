import { useInfiniteQuery } from "@tanstack/react-query";
import { useMemo } from "react";

import type { Api } from "@/lib/api";
import type { Components } from "@/lib/generated/contracts";

import { Notice } from "@/components/notice";
import { SearchableChoice } from "@/components/searchable-choice";
import { Button } from "@/components/ui/button";
type Account = Components["schemas"]["LinkedAccount"];
type Page = Components["schemas"]["LinkedAccountPage"];

export function AccountPicker({
  api,
  configured,
  id,
  onChange,
  plexId,
}: Readonly<{
  api: Api;
  configured: boolean;
  id: string;
  plexId: string;
  onChange: (value: Account | null) => void;
}>) {
  const loadAccounts = async ({
    pageParam,
    signal,
  }: {
    pageParam: number;
    signal: AbortSignal;
  }): Promise<Page> =>
    (
      await api.read<Page>(
        `/v1/config/accounts?offset=${pageParam}`,
        false,
        signal,
      )
    ).data;
  const query = useInfiniteQuery({
    enabled: configured,
    initialPageParam: 0,
    queryFn: loadAccounts,
    queryKey: ["linked-accounts"],
    retry: false,
    staleTime: 30_000,
    // Pagination callbacks follow queryFn for TanStack's type inference.
    getNextPageParam: (last: Page) => last.next_offset ?? undefined,
  });
  const accounts = useMemo(
    () => query.data?.pages.flatMap((page) => page.items) ?? [],
    [query.data],
  );
  const options = [
    { label: "No requester account", value: "none" },
    ...accounts.map((account) => ({
      label: account.name,
      value: String(account.id),
    })),
  ];

  if (id && !accounts.some((account) => String(account.id) === id)) {
    options.push({ label: `Current Seerr account #${id}`, value: id });
  }

  if (!configured) {
    return (
      <p className="text-sm text-muted-foreground">
        Configure Seerr in Connections to link a requester account.
      </p>
    );
  }

  return (
    <div className="grid gap-2">
      <span className="text-sm font-medium">Requester account</span>
      <SearchableChoice
        label="Requester account"
        value={id || "none"}
        options={options}
        onChange={(value) => {
          if (value !== id) {
            onChange(
              accounts.find((account) => String(account.id) === value) ?? null,
            );
          }
        }}
        className="w-full"
      />
      {query.error && <Notice>{query.error.message}</Notice>}
      {query.hasNextPage && (
        <Button
          type="button"
          variant="outline"
          size="sm"
          disabled={query.isFetchingNextPage}
          onClick={() => void query.fetchNextPage()}
        >
          Load more accounts
        </Button>
      )}
      {id && (
        <p className="text-xs text-muted-foreground">
          Seerr #{id}, Plex #{plexId}
        </p>
      )}
    </div>
  );
}
