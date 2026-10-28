/**
 * Forwarding into Durable Objects. DOs trust the identity headers stamped
 * here blindly — they are only reachable through the Worker (design §2: "DO
 * never sees an unauthenticated frame") — so every Worker-controlled header
 * is cleared from the inbound request before the verified values are set.
 */
import type { Verified } from "./auth";
import { AUTH_USER_HEADER, ROOM_KIND_HEADER } from "./env";

const ID_RE = /^[A-Za-z0-9_-]{1,128}$/;

/** Forward into a DO with the verified caller stamped on the request. */
export const forward = (
  ns: DurableObjectNamespace,
  name: string,
  request: Request,
  auth: Verified,
  path: string,
  search?: string,
  roomKind?: "workspace"
): Promise<Response> => {
  const stub = ns.get(ns.idFromName(name));
  const url = new URL(request.url);
  url.pathname = path;
  if (search !== undefined) url.search = search;
  const headers = new Headers(request.headers);
  // room-kind is a Worker-controlled signal (the DO relaxes owner gating for
  // workspace rooms): clear any inbound value so only the explicit set below —
  // reached solely on workspace forwards, after the org-membership check —
  // can assert it. Do not drop this line; passthrough would let a caller
  // choose their own room kind.
  headers.delete(ROOM_KIND_HEADER);
  headers.set(AUTH_USER_HEADER, auth.userId);
  if (roomKind) headers.set(ROOM_KIND_HEADER, roomKind);
  // Fork (D1, sync d5a01522): no runner bearers here, so upstream's
  // RUNNER_DEVICE/RUNNER_ACCOUNT stamping stays with the dormant Cloud code.
  return stub.fetch(new Request(url.toString(), { method: request.method, body: request.body, headers }));
};

/** Carry the dialing engine's `&device=` through to the DO (socket
 * attribution in logs — the 2026-08-04 deaf socket was only identifiable by
 * reverse-engineering rotating IPv6 privacy addresses). Validated so a
 * hand-crafted value can't inject into log lines or the DO's query. */
export const deviceParam = (url: URL): string => {
  const device = url.searchParams.get("device") ?? "";
  return ID_RE.test(device) ? `&device=${device}` : "";
};
