import { configDefaults, defineConfig } from "vitest/config";

// Pure-logic unit tests (Node, FakeSql). The workerd tier lives in
// vitest.workerd.config.ts and must not be picked up here: its tests import
// `cloudflare:test`, which only resolves inside the workers pool.
// test/unit holds Node tests that need Node APIs (e.g. running the Cloud
// install script under bash) — outside src/ so the Workers typecheck never
// sees `node:*` imports.
export default defineConfig({
  test: {
    include: ["src/**/*.test.ts", "test/unit/**/*.test.ts"],
    // Fork (D1, sync d5a01522): upstream Cloud/runner/vault code is dormant
    // (not imported by src/index.ts, not typechecked), so its tests stay out
    // of the run. Re-enable together with the Cloud wiring.
    exclude: [
      ...configDefaults.exclude,
      "src/cloud/**",
      "src/runner-*.test.ts",
      "src/registry-runner.test.ts",
      "test/unit/cloud-access.test.ts",
      "test/unit/install-script.test.ts",
      "test/unit/sandbox-providers.test.ts"
    ]
  }
});
