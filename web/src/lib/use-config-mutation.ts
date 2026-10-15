"use client";

import {
  useIsMutating,
  useMutation,
  useQueryClient,
} from "@tanstack/react-query";

import type { Api } from "@/lib/api";
import type { Components as components } from "@/lib/generated/contracts";
import type { SettingsDocument, SettingsEnvelope } from "@/lib/settings";

import { HttpError } from "@/lib/api";

const configWrites = ["/v1/config"];

/** A conflict never retries a stale document or silently overwrites another editor. */
export function useConfigMutation<T = FormData>(
  api: Api,
  document: SettingsEnvelope,
  serialize: (form: T) => SettingsDocument,
  invalidate: string[] = [],
  onSaved?: (restartRequired: boolean) => void,
) {
  const cache = useQueryClient();
  const pending = useIsMutating({ mutationKey: configWrites }) > 0;
  const mutation = useMutation({
    mutationFn: async (form: T) =>
      api.write<components["schemas"]["ConfigUpdateResult"]>("/v1/config", {
        revision: document.revision,
        settings: serialize(form),
      }),
    mutationKey: configWrites,
    onSuccess: async (response) => {
      onSaved?.(response.data.restart_required);
      await Promise.all(
        ["/v1/config", "/v1/status", ...invalidate].map((path) =>
          cache.invalidateQueries({ queryKey: [path] }),
        ),
      );
    },
  });

  return {
    ...mutation,
    conflict:
      mutation.error instanceof HttpError && mutation.error.status === 409,
    isPending: pending,
    reload: () => cache.invalidateQueries({ queryKey: ["/v1/config"] }),
  };
}
