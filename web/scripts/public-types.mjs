import { mkdir, readFile, writeFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import openapiTS, { astToString } from "openapi-typescript";
import { format, resolveConfig } from "prettier";

import { localReferences } from "./schema.mjs";

const args = new Set(process.argv.slice(2));

if ([...args].some((arg) => arg !== "--check")) {
  throw new Error("Supported option: --check");
}

const input = new URL("../../docs/api/public.schema.json", import.meta.url);
const output = new URL("../src/lib/generated/contracts.d.ts", import.meta.url);
const boundsOutput = new URL(
  "../src/lib/generated/form-constraints.ts",
  import.meta.url,
);
const schema = JSON.parse(await readFile(input, "utf8"));

localReferences(schema);

const ast = await openapiTS(schema, { alphabetize: true });
const generated = await format(
  "// Generated from Rust public DTOs; run pnpm types. Do not edit.\n" +
    astToString(ast) +
    "\nexport type Components = components;\n",
  { ...(await resolveConfig(fileURLToPath(output))), parser: "typescript" },
);

const numericFields = Object.fromEntries(
  Object.entries({
    cache: "CachePolicy",
    mcp: "Limits",
    subscriptions: "SubscriptionPolicy",
  }).map(([group, name]) => [
    group,
    Object.fromEntries(
      Object.entries(schema.components.schemas[name].properties)
        .filter(([, property]) => property.type === "integer")
        .map(([name, property]) => {
          if (
            ![property.minimum, property.maximum, property.default].every(
              (value) => Number.isSafeInteger(value),
            )
          ) {
            throw new Error(`Missing numeric form bounds: ${group}.${name}`);
          }

          return [
            name,
            {
              default: property.default,
              max: property.maximum,
              min: property.minimum,
            },
          ];
        }),
    ),
  ]),
);
const bounds = await format(
  "// Generated from Rust configuration schemas; run pnpm types. Do not edit.\n" +
    `export const numericFields = ${JSON.stringify(numericFields)} as const;\n`,
  {
    ...(await resolveConfig(fileURLToPath(boundsOutput))),
    parser: "typescript",
  },
);
const outputs = [
  [output, generated],
  [boundsOutput, bounds],
];

for (const [path, content] of outputs) {
  if (args.has("--check")) {
    let existing;

    try {
      existing = await readFile(path, "utf8");
    } catch (error) {
      if (error.code !== "ENOENT") {
        throw error;
      }
    }

    if (existing !== content) {
      throw new Error("ArrPi public contract drift: run pnpm types");
    }
  } else {
    await mkdir(new URL(".", path), { recursive: true });
    await writeFile(path, content);
  }
}

console.log(
  `${args.has("--check") ? "Verified" : "Generated"} ArrPi public contracts`,
);
