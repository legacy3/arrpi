import { Search, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { cn } from "@/lib/utils";

export function SearchField({
  className,
  label = "Search",
  onChange,
  value,
}: Readonly<{
  value: string;
  onChange: (value: string) => void;
  label?: string;
  className?: string;
}>) {
  return (
    <div className={cn("relative min-w-0", className)}>
      <Search
        aria-hidden
        className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground"
      />
      <Input
        type="search"
        aria-label={label}
        onKeyDown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
          }
        }}
        placeholder={label}
        value={value}
        maxLength={128}
        onChange={(event) => onChange(event.target.value)}
        className="pl-9 pr-9 [&::-webkit-search-cancel-button]:appearance-none"
      />
      {value && (
        <Button
          type="button"
          variant="ghost"
          size="icon-sm"
          aria-label={`Clear ${label.toLowerCase()}`}
          className="absolute right-1 top-1/2 -translate-y-1/2"
          onClick={() => onChange("")}
        >
          <X className="size-3.5" />
        </Button>
      )}
    </div>
  );
}
