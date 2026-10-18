import { useBoolean, useMount, useUpdateEffect } from "ahooks";
import { ArrowLeft, Plus, Trash2 } from "lucide-react";
import Link from "next/link";
import { usePathname, useSearchParams } from "next/navigation";
import { parseAsString, useQueryState } from "nuqs";
import { type ReactNode, useRef } from "react";

import type { SettingsDocument } from "@/lib/settings";

import { ConfigConflict } from "@/components/config-conflict";
import { Notice } from "@/components/notice";
import { SearchField } from "@/components/search-field";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { useFuzzySearch } from "@/hooks/use-fuzzy-search";
import { useConfigMutation } from "@/lib/use-config-mutation";

import type { EditorProps } from "./form";
const ENTRY_KEYS = ["name", "detail", "id"] as const;

export function EntryEditor({
  children,
  title,
}: Readonly<{ title: string; children: ReactNode }>) {
  const path = usePathname();
  const params = new URLSearchParams(useSearchParams().toString());

  params.delete("entry");
  const href = params.size > 0 ? `${path}?${params}` : path;

  return (
    <div className="grid max-w-3xl gap-4">
      <Button
        type="button"
        variant="ghost"
        className="justify-self-start"
        asChild
      >
        <Link href={href} prefetch={false}>
          <ArrowLeft className="size-4" />
          Back
        </Link>
      </Button>
      <EntryHeading>{title}</EntryHeading>
      {children}
    </div>
  );
}

export function EntryList({
  addDisabled = false,
  addLabel,
  entries,
  focusOnMount = true,
  message,
  onAdd,
  onSelect,
  title,
}: Readonly<{
  message?: string;
  title: string;
  addLabel: string;
  addDisabled?: boolean;
  focusOnMount?: boolean;
  onAdd: () => void;
  entries: {
    id: string;
    name: string;
    detail: string;
    editLabel?: string;
    status?: ReactNode;
  }[];
  onSelect: (id: string) => void;
}>) {
  const [query, setQuery] = useQueryState(
    "search",
    parseAsString.withDefault(""),
  );
  const matches = useFuzzySearch({ items: entries, keys: ENTRY_KEYS, query });

  return (
    <Card className="gap-0 py-0">
      <div className="flex items-center justify-between gap-3 p-5">
        <EntryHeading focusOnMount={focusOnMount}>{title}</EntryHeading>
        <Button
          type="button"
          variant="outline"
          size="sm"
          onClick={onAdd}
          disabled={addDisabled}
        >
          <Plus className="size-4" />
          {addLabel}
        </Button>
      </div>
      {message && (
        <p role="status" className="border-t px-5 py-3 text-sm text-primary">
          {message}
        </p>
      )}
      <div className="px-5 pb-4">
        <SearchField
          label={`Search ${title.toLowerCase()}`}
          value={query}
          onChange={(value) => void setQuery(value)}
        />
      </div>
      <ul className="divide-y border-t">
        {matches.map((entry) => (
          <li
            key={entry.id}
            className="flex items-center justify-between gap-4 px-5 py-3"
          >
            <div className="min-w-0">
              <p className="truncate text-sm font-medium">{entry.name}</p>
              <p className="truncate text-xs text-muted-foreground">
                {entry.detail}
              </p>
            </div>
            {entry.status}
            <Button
              type="button"
              variant="ghost"
              size="sm"
              aria-label={entry.editLabel ?? `Edit ${entry.name}`}
              onClick={() => onSelect(entry.id)}
            >
              Edit
            </Button>
          </li>
        ))}
      </ul>
      {matches.length === 0 && (
        <p className="px-5 py-5 text-sm text-muted-foreground">
          No matching entries.
        </p>
      )}
    </Card>
  );
}

export function RemoveEntry({
  blocked,
  focusOnMount = false,
  hint,
  label,
  settings,
  ...props
}: EditorProps &
  Readonly<{
    blocked?: string;
    focusOnMount?: boolean;
    hint?: string;
    label: string;
    settings: SettingsDocument;
  }>) {
  const [confirm, { setFalse, setTrue }] = useBoolean();
  const cancelButtonRef = useRef<HTMLButtonElement>(null);
  const removeButtonRef = useRef<HTMLButtonElement>(null);

  useMount(() => {
    if (focusOnMount) {
      removeButtonRef.current?.focus();
    }
  });

  useUpdateEffect(() => {
    (confirm ? cancelButtonRef : removeButtonRef).current?.focus();
  }, [confirm]);
  const mutation = useConfigMutation<void>(
    props.api,
    props.document,
    () => settings,
    [],
    props.onSaved,
  );

  return (
    <div className="grid justify-items-start gap-3 border-t pt-4">
      {hint && <p className="text-xs text-muted-foreground">{hint}</p>}
      {blocked && <p className="text-xs text-muted-foreground">{blocked}</p>}
      {mutation.conflict && <ConfigConflict reload={mutation.reload} />}
      {mutation.error && !mutation.conflict && (
        <Notice>{mutation.error.message}</Notice>
      )}
      {confirm ? (
        <div className="flex flex-wrap items-center gap-3">
          <p className="text-sm">Remove {label} from ArrPi?</p>
          <Button
            type="button"
            variant="destructive"
            size="sm"
            disabled={
              Boolean(blocked) || mutation.isPending || mutation.conflict
            }
            onClick={() => mutation.mutate()}
          >
            Confirm removal
          </Button>
          <Button
            ref={cancelButtonRef}
            type="button"
            variant="ghost"
            size="sm"
            onClick={setFalse}
          >
            Cancel
          </Button>
        </div>
      ) : (
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="text-destructive"
          ref={removeButtonRef}
          disabled={Boolean(blocked) || mutation.isPending}
          onClick={() => {
            props.onEdit();
            setTrue();
          }}
        >
          <Trash2 className="size-4" />
          Remove {label}
        </Button>
      )}
    </div>
  );
}

function EntryHeading({
  children,
  focusOnMount = true,
}: Readonly<{ children: ReactNode; focusOnMount?: boolean }>) {
  const headingRef = useRef<HTMLHeadingElement>(null);

  useMount(() => {
    if (focusOnMount) {
      headingRef.current?.focus();
    }
  });

  return (
    <h2
      ref={headingRef}
      tabIndex={-1}
      className="scroll-mt-24 font-semibold outline-none"
    >
      {children}
    </h2>
  );
}
