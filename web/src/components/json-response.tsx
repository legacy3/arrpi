import { Textarea } from "@/components/ui/textarea";

export function JsonResponse({
  label = "JSON response",
  value,
}: Readonly<{ label?: string; value: unknown }>) {
  return (
    <Textarea
      readOnly
      wrap="off"
      value={JSON.stringify(value, null, 2)}
      aria-label={label}
      className="h-80 max-h-[70vh] field-sizing-fixed w-full resize-y overflow-auto rounded-none rounded-b-xl border-0 bg-background/30 p-5 font-mono text-xs leading-7 text-emerald-100/80 shadow-none focus-visible:ring-inset"
    />
  );
}
