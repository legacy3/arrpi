import { Search, X } from "lucide-react";

import {
  InputGroup,
  InputGroupAddon,
  InputGroupButton,
  InputGroupInput,
} from "@/components/ui/input-group";
import { cn } from "@/lib/utils";

export function SearchField({
  className,
  label = "Search",
  onChange,
  placeholder,
  value,
}: Readonly<{
  value: string;
  onChange: (value: string) => void;
  label?: string;
  placeholder?: string;
  className?: string;
}>) {
  return (
    <InputGroup className={cn("min-w-0", className)}>
      <InputGroupAddon>
        <Search aria-hidden />
      </InputGroupAddon>
      <InputGroupInput
        type="search"
        aria-label={label}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
          }
        }}
        placeholder={placeholder ?? label}
        value={value}
        maxLength={128}
        onChange={(event) => onChange(event.target.value)}
        className="[&::-webkit-search-cancel-button]:appearance-none"
      />
      {value && (
        <InputGroupAddon align="inline-end">
          <InputGroupButton
            type="button"
            variant="ghost"
            size="icon-xs"
            aria-label={`Clear ${label.toLowerCase()}`}
            onClick={() => onChange("")}
          >
            <X className="size-3.5" />
          </InputGroupButton>
        </InputGroupAddon>
      )}
    </InputGroup>
  );
}
