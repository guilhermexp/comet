import { chatRoute } from "../../src/chat-route";
import { DurableObject } from "cloudflare:workers";
import { authenticate } from "../../src/auth";
import type { Env } from "../../src/env";
import { deviceParam, forward } from "../../src/forward";
import { previewRoute } from "../../src/preview-route";
import { releaseRoute } from "../../src/releases";
export { DeviceRoom } from "../../src/device-room";
export { ChatRoom } from "../../src/chat-room";
export { PreviewRoom } from "../../src/preview-room";
export { RegistryRoom } from "../../src/registry-room";

/** Bare SQLite-backed DO; tests reach its real `ctx.storage.sql` via
 * `runInDurableObject` (the cloudflare-os TEST_OVERSEER pattern). */
export class TestLogRoom extends DurableObject {}

const json = (value: unknown, status = 200) =>
  new Response(JSON.stringify(value), { status, headers: { "content-type": "application/json" } });

/**
 * The production routing seam, in src/index.ts's order, minus the routes
 * that need loro's WASM (which cannot load in this tier): releases, bearer
 * gate, preview, /device, /registry, /chat2 and /blob — all through the
 * production `forward`. Fork (D1, sync d5a01522): without upstream's Cloud,
 * vault and runner routes.
 */
export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    const release = await releaseRoute(request, env, url);
    if (release) return release;
    const auth = await authenticate(env, request);
    if (!auth) return new Response("Unauthorized", { status: 401 });
    const preview = previewRoute(request, env, auth);
    if (preview) return preview;
    const parts = url.pathname.split("/").filter(Boolean);
    const id = /^[A-Za-z0-9_-]{1,128}$/;
    if (parts[0] === "device" && parts[1] && id.test(parts[1])) {
      const room = `d2/${parts[1]}`;
      if (parts[2] === "ws") {
        const role = url.searchParams.get("role") === "host" ? "host" : "client";
        return forward(env.DEVICE_ROOMS, room, request, auth, "/ws", `?role=${role}&connId=${crypto.randomUUID()}`);
      }
      if (parts[2] === "nudge" && request.method === "POST") return forward(env.DEVICE_ROOMS, room, request, auth, "/nudge", "");
      if (parts[2] === "status") return forward(env.DEVICE_ROOMS, room, request, auth, "/status", "");
      if (parts[2] === "sidecar" && parts[3]) return forward(env.DEVICE_ROOMS, room, request, auth, `/sidecar/${parts[3]}`, "");
    }
    if (parts[0] === "registry" && parts[1] === auth.orgId) {
      const room = `reg1/${parts[1]}/${auth.userId}`;
      if (parts[2] === "ws") return forward(env.REGISTRY_ROOMS, room, request, auth, "/ws", `?${deviceParam(url).replace(/^&/, "")}`);
      if (parts[2] === "rows" || parts[2] === "push" || parts[2] === "stats") {
        return forward(env.REGISTRY_ROOMS, room, request, auth, `/${parts[2]}`, url.search);
      }
    }
    const chat = chatRoute(request, env, auth.userId);
    if (chat) return chat;
    if (parts[0] === "blob" && parts[1] && parts[2]) {
      const key = `blob/${auth.userId}/${parts[1]}/${parts[2]}`;
      if (request.method === "PUT") {
        await env.BLOBS.put(key, await request.arrayBuffer());
        return json({ ok: true });
      }
      const object = await env.BLOBS.get(key);
      return object ? new Response(object.body) : json({ error: "not_found" }, 404);
    }
    return new Response("test fixture", { status: 404 });
  }
};
