import { createConfig, presets } from "@wowlab/eslint-config";

export default createConfig({
  ignores: [
    "dist/**",
    ".next/**",
    "out/**",
    "next-env.d.ts",
    "src/lib/generated/**",
    "test-results/**",
    "playwright-report/**",
  ],
  internalPatterns: ["^@/"],
  presets: [
    presets.next(),
    presets.react({ tsconfigRootDir: import.meta.dirname, typeChecked: true }),
    presets.node(),
    presets.tanstackQuery(),
  ],
});
