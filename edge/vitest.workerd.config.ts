import { configDefaults, defineConfig } from "vitest/config";
import { cloudflareTest } from "@cloudflare/vitest-pool-workers";

// Runtime-real test tier: runs inside actual workerd via
// @cloudflare/vitest-pool-workers, against a real SQLite-backed Durable
// Object, so platform limits like the ~2MB SQLITE_TOOBIG row cap (the
// 2026-08-05 whale sync freeze) are the runtime's own, not FakeSql constants.
// `npm run test:workerd`.
//
// loro-wasm does NOT work in this tier: the pool's test runner evaluates
// modules where wasm codegen is disallowed, while a real worker compiles the
// base64-inlined module at startup (deployed edge and `wrangler dev` are
// fine). loro-on-workerd coverage lives in the wrangler-dev scripts
// (scripts/whale-check.mjs, scripts/fold-check.mjs).
//
// Fork (D1, sync d5a01522): no Cloud tier. Upstream's CloudAccount/CloudIndex
// DOs, Workflows, VAULT service binding, Boat/Stripe fakes and runner JWT keys
// are not configured here; the tests that need them are excluded below.
export default defineConfig({
  plugins: [
    cloudflareTest({
      main: "./test/workerd/fixture.ts",
      miniflare: {
        compatibilityDate: "2026-07-01",
        r2Buckets: ["BLOBS", "RELEASES"],
        durableObjects: {
          DEVICE_ROOMS: { className: "DeviceRoom", useSQLite: true },
          TEST_LOG: { className: "TestLogRoom", useSQLite: true },
          CHAT_ROOMS: { className: "ChatRoom", useSQLite: true },
          PREVIEW_ROOMS: { className: "PreviewRoom", useSQLite: true },
          REGISTRY_ROOMS: { className: "RegistryRoom", useSQLite: true }
        },
        // The fixture authenticates through the production `authenticate`
        // (dev mode: the bearer `userId@orgId` is the identity).
        bindings: {
          AUTH_MODE: "dev",
          WORKOS_CLIENT_ID: "test-only"
        }
      }
    })
  ],
  resolve: {
    // Mirror wrangler.jsonc: workerd cannot fetch loro's WASM by URL; the
    // base64 entry inlines it.
    alias: { "loro-crdt": "loro-crdt/base64" }
  },
  test: {
    include: ["test/workerd/**/*.test.ts"],
    // Fork (D1): dormant Cloud/runner/vault suites (they need the bindings
    // above that the fork does not configure).
    exclude: [
      ...configDefaults.exclude,
      "test/workerd/cloud*.workerd.test.ts",
      "test/workerd/runner-boundary.workerd.test.ts",
      "test/workerd/vault-routes.workerd.test.ts"
    ]
  }
});
