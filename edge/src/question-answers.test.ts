import { describe, expect, it } from "vitest";
import { toDocParts, fromDocParts } from "./session-doc/messages";

describe("question answer history", () => {
  it("preserves answers through both document projections", () => {
    const part = { kind: "input" as const, requestId: "r", questions: [], resolved: true,
      answers: [{ questionId: "q", labels: ["Café", "Silêncio"] }] };
    expect(fromDocParts(toDocParts([part]))).toEqual([part]);
  });
  it("does not invent answers for older documents", () => {
    expect(fromDocParts([{ id: "r", kind: "input", questions: [], resolved: true }])[0]).not.toHaveProperty("answers");
  });
});
