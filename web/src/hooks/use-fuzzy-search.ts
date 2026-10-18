"use client";

import MiniSearch from "minisearch";
import { useMemo } from "react";

const INDEX_FIELD = "__fuzzySearchIndex__";

type SearchDoc = Record<string, string>;

type UseFuzzySearchParams<T> = {
  items: readonly T[];
  query: string;
  keys: readonly (keyof T & string)[];
  limit?: number;
};

export function useFuzzySearch<T>({
  items,
  keys,
  limit = 200,
  query,
}: UseFuzzySearchParams<T>) {
  const normalizedQuery = query.trim();

  const index = useMemo(() => {
    const docs: SearchDoc[] = items.map((item, i) => {
      const doc: SearchDoc = { [INDEX_FIELD]: String(i) };

      for (const key of keys) {
        const value = item[key];

        doc[key] = typeof value === "string" ? value : String(value ?? "");
      }

      return doc;
    });

    const miniSearch = new MiniSearch<SearchDoc>({
      fields: [...keys],
      idField: INDEX_FIELD,
      searchOptions: {
        fuzzy: 0.2,
        prefix: true,
      },
    });

    miniSearch.addAll(docs);

    return { miniSearch };
  }, [items, keys]);

  return useMemo(() => {
    if (!normalizedQuery) {
      return [...items];
    }

    const results = index.miniSearch.search(normalizedQuery).slice(0, limit);

    return results
      .map((result) => items[Number(result.id)])
      .filter((item): item is T => item != null);
  }, [index, items, limit, normalizedQuery]);
}
