import { spawn } from "node:child_process";

const notifyPath = {{NOTIFY_PATH_JSON}};

function providerSessionMetadata(ctx) {
  const manager = ctx?.sessionManager;
  const sessionId = manager?.getSessionId?.();
  const transcriptPath = manager?.getSessionFile?.();
  return {
    ...(typeof sessionId === "string" && sessionId
      ? { session_id: sessionId }
      : {}),
    ...(typeof transcriptPath === "string" && transcriptPath
      ? { provider_transcript_path: transcriptPath }
      : {}),
  };
}

function promptToolName(event) {
  const kind = typeof event?.kind === "string" ? event.kind.trim() : "";
  if (!kind || kind === "custom") {
    return "AskUserQuestion";
  }
  return kind;
}

function notify(hookEventName, ctx, toolName) {
  return new Promise((resolve) => {
    const payload = {
      hook_event_name: hookEventName,
      ...providerSessionMetadata(ctx),
      ...(typeof toolName === "string" && toolName
        ? { tool_name: toolName }
        : {}),
    };
    const child = spawn(
      "bash",
      [notifyPath, JSON.stringify(payload)],
      { stdio: "ignore" },
    );
    child.once("error", resolve);
    child.once("exit", resolve);
  });
}

export default function registerUnpeelLifecycle(extension) {
  extension.on("agent_start", async (_event, ctx) => {
    await notify("Start", ctx);
  });
  extension.on("agent_end", async (_event, ctx) => {
    await notify("Stop", ctx);
  });
  extension.on("ui_prompt_start", async (event, ctx) => {
    await notify("PermissionRequest", ctx, promptToolName(event));
  });
  extension.on("ui_prompt_end", async (_event, ctx) => {
    await notify("UserPromptSubmit", ctx);
  });
}
