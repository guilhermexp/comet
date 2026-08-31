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

function notify(hookEventName, ctx) {
  return new Promise((resolve) => {
    const payload = {
      hook_event_name: hookEventName,
      ...providerSessionMetadata(ctx),
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
}
