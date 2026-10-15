export function coreOrigin(value?: string) {
  let origin: URL;

  try {
    origin = new URL(value ?? "http://127.0.0.1:8780");
  } catch {
    throw new Error("Invalid ArrPi core origin");
  }

  if (
    !["http:", "https:"].includes(origin.protocol) ||
    origin.username ||
    origin.password ||
    origin.pathname !== "/" ||
    origin.search ||
    origin.hash
  ) {
    throw new Error("Invalid ArrPi core origin");
  }

  return origin;
}
