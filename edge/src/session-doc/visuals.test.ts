import { describe, expect, it } from "vitest";
import { type DocMessagePart, fromDocParts, toDocParts, splitMessageEntry, joinContinuations } from "./messages";
import { type SessionMessagePart, toRenderParts } from "./render-parts";

describe("native visual parts", () => {
  const visual = { kind: "visual", id: "v:1", path: "/uploads/v-1-visual.luau", title: "Latency" } as unknown as SessionMessagePart;
  it("preserves the script reference through render/doc conversion", () => {
    expect(fromDocParts(toDocParts(toRenderParts([visual])))).toEqual([visual]);
  });
  it("keeps visual references atomic through split/join", () => {
    const part = { kind: "visual", id: "v:1", path: "/uploads/v-1-visual.luau", name: "Latency" } as unknown as DocMessagePart;
    const entry = { id: "a", role: "assistant" as const, deviceId: "owner", createdAt: 1, parts: [part, { ...part, id: "v:2" }] };
    const split = splitMessageEntry(entry, 110);
    expect(split).toHaveLength(2);
    expect(joinContinuations(split)).toEqual([entry]);
  });
  it("degrades invalid references without exposing source or paths as prose", () => {
    for (const path of [undefined, "", 42]) {
      const part = { kind: "visual", id: "v:1", path, text: "private source" } as unknown as DocMessagePart;
      expect(fromDocParts([part])).toEqual([{ kind: "error", id: "v:1", message: "Visual unavailable" }]);
    }
  });
});
