import { FieldLabel, Field as FormField } from "@/components/ui/field";
import { Input } from "@/components/ui/input";

export function Field({
  name,
  id = name,
  label,
  ...props
}: Readonly<
  { label: string; name: string } & React.ComponentProps<typeof Input>
>) {
  return (
    <FormField data-invalid={props["aria-invalid"]}>
      <FieldLabel htmlFor={id}>{label}</FieldLabel>
      <Input id={id} name={name} autoComplete="off" {...props} />
    </FormField>
  );
}
