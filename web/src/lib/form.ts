export function formInteger(
  form: FormData,
  name: string,
  bounds?: { min: number; max?: number },
) {
  const text = formText(form, name);
  const value = Number(text);

  if (
    !text ||
    !Number.isSafeInteger(value) ||
    value < (bounds?.min ?? 0) ||
    value > (bounds?.max ?? Number.MAX_SAFE_INTEGER)
  ) {
    throw new Error(`Enter a whole number for ${name.replaceAll("_", " ")}`);
  }

  return value;
}

/** Form primitives for the MCP configuration editor. */
export function formText(form: FormData, name: string) {
  const value = form.get(name);

  return typeof value === "string" ? value.trim() : "";
}
