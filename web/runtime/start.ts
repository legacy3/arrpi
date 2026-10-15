import { coreOrigin } from "./core-origin.ts";

coreOrigin(process.env.ARRPI_CORE_ORIGIN);

const standalone = new URL("../server.js", import.meta.url);

await import(standalone.href);
