import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

export function Field({
  label,
  name,
  ...props
}: Readonly<
  { label: string; name: string } & React.ComponentProps<typeof Input>
>) {
  return (
    <div className="grid gap-2">
      <Label htmlFor={name}>{label}</Label>
      <Input id={name} name={name} autoComplete="off" {...props} />
    </div>
  );
}
