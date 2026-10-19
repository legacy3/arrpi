"use client";

import { useBoolean, useResetState } from "ahooks";
import { Check, ChevronsUpDown } from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Command,
  CommandEmpty,
  CommandInput,
  CommandItem,
  CommandList,
} from "@/components/ui/command";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { useFuzzySearch } from "@/hooks/use-fuzzy-search";
import { cn } from "@/lib/utils";
const KEYS = ["label", "value"] as const;

type Option = { value: string; label: string };

export function SearchableChoice({
  className,
  disabled = false,
  id,
  label,
  onChange,
  options,
  value,
}: Readonly<{
  label: string;
  value: string;
  options: readonly Option[];
  onChange: (value: string) => void;
  className?: string;
  id?: string;
  disabled?: boolean;
}>) {
  const [open, { set: setOpen, setFalse: close }] = useBoolean();
  const [query, setQuery, reset] = useResetState("");
  const matches = useFuzzySearch({
    items: options,
    keys: KEYS,
    limit: 500,
    query,
  });

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button
          type="button"
          id={id}
          role="combobox"
          aria-label={label}
          aria-expanded={open}
          variant="outline"
          disabled={disabled}
          className={cn("min-w-0 justify-between font-normal", className)}
        >
          <span className="truncate">
            {options.find((option) => option.value === value)?.label ??
              `Choose ${label.toLowerCase()}`}
          </span>
          <ChevronsUpDown
            aria-hidden
            className="ml-2 size-3.5 shrink-0 text-muted-foreground"
          />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        className="w-(--radix-popover-trigger-width) min-w-56 max-w-[calc(100vw-2rem)] p-0"
        onCloseAutoFocus={(event) => {
          const active = document.activeElement;

          // A closing picker must not steal focus from the next control.
          if (
            active &&
            active !== document.body &&
            event.target instanceof Node &&
            !event.target.contains(active)
          ) {
            event.preventDefault();
          }
        }}
      >
        <Command shouldFilter={false}>
          <CommandInput
            aria-label={`Search ${label.toLowerCase()}`}
            placeholder={`Search ${label.toLowerCase()}...`}
            value={query}
            onValueChange={setQuery}
          />
          <CommandList>
            <CommandEmpty>No matches.</CommandEmpty>
            {matches.map((option) => (
              <CommandItem
                key={option.value}
                value={option.value}
                onSelect={() => {
                  onChange(option.value);
                  close();
                  reset();
                }}
              >
                <span className="min-w-0 flex-1 truncate">{option.label}</span>
                {value === option.value && <Check className="size-4" />}
              </CommandItem>
            ))}
          </CommandList>
        </Command>
      </PopoverContent>
    </Popover>
  );
}
