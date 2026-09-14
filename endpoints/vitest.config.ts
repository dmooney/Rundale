import { defineConfig } from "vitest/config";
import { fileURLToPath } from "node:url";

export default defineConfig({
  // CI starts from an artifact-free checkout. Resolve workspace packages to
  // source explicitly so tests do not accidentally depend on ignored dist/ output.
  resolve: {
    conditions: ["source"],
    alias: Object.fromEntries(
      ["auth", "database", "domain", "observability", "providers", "runtime", "schemas"].map(
        (name) => [
          `@parish/${name}`,
          fileURLToPath(new URL(`./packages/${name}/src/index.ts`, import.meta.url)),
        ],
      ),
    ),
  },
  test: {
    include: ["{apps,packages,examples}/**/*.test.ts"],
    coverage: {
      provider: "v8",
      reporter: ["text", "json-summary"],
    },
  },
});
