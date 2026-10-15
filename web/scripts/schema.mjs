/** Refuse remote references so committed schema generation stays offline and reproducible. */
export function localReferences(value) {
  if (value && typeof value === "object") {
    for (const [key, child] of Object.entries(value)) {
      if (
        key === "$ref" &&
        (typeof child !== "string" || !child.startsWith("#/"))
      ) {
        throw new Error("External schema references are not allowed");
      }

      localReferences(child);
    }
  }
}
