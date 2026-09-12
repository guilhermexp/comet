import { expect, test } from "vitest";
import { sanitizeToolCall } from "./session-doc/render-parts";

test("skill render input keeps identifiers and drops private arguments", () => {
  for (const name of ["Skill", "skill"]) {
    const clean = sanitizeToolCall({ _tag: "Unknown", name, input: {
      skill: " implement ", path: "brainstorming", name: "review",
      args: "private arguments", prompt: "private prompt", content: "private content"
    }});
    expect(clean).toEqual({ _tag: "Unknown", name, input: {
      skill: "implement", path: "brainstorming", name: "review"
    }});
    expect(sanitizeToolCall(clean)).toEqual(clean);
  }
  expect(sanitizeToolCall({ _tag: "Unknown", name: "other", input: {skill:"private"} }))
    .toEqual({_tag:"Unknown", name:"other"});
});
