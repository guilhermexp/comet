import { realpathSync, statSync } from "node:fs";
import { dirname } from "node:path";
import { spawn } from "node:child_process";

const notifyPath = {{NOTIFY_PATH_JSON}};
const runtimeId = {{RUNTIME_ID_JSON}};
const filterNestedOmpSubagents = {{FILTER_NESTED_OMP_SUBAGENTS_JSON}};

function providerSessionMetadata(ctx) {
  const manager = ctx?.sessionManager;
  const sessionId = manager?.getSessionId?.();
  const transcriptPath = manager?.getSessionFile?.();
  const agentKind = ctx?.agent?.kind;
  return {
    ...(typeof agentKind === "string" && agentKind
      ? { unpeel_agent_kind: agentKind }
      : {}),
    ...(typeof sessionId === "string" && sessionId
      ? { session_id: sessionId }
      : {}),
    ...(typeof transcriptPath === "string" && transcriptPath
      ? { provider_transcript_path: transcriptPath }
      : {}),
  };
}

function isNestedOmpAgent(ctx) {
  if (ctx?.agent?.kind === "sub") {
    return true;
  }
  if (ctx?.agent?.kind === "main") {
    return false;
  }
  if (!filterNestedOmpSubagents) {
    return false;
  }

  const transcriptPath = ctx?.sessionManager?.getSessionFile?.();
  if (typeof transcriptPath !== "string" || !transcriptPath.endsWith(".jsonl")) {
    return false;
  }

  let childTranscriptPath = transcriptPath;
  try {
    childTranscriptPath = realpathSync(transcriptPath);
  } catch {}

  try {
    return statSync(`${dirname(childTranscriptPath)}.jsonl`).isFile();
  } catch {
    return false;
  }
}
function promptToolName(event) {
  const kind = typeof event?.kind === "string" ? event.kind.trim() : "";
  if (!kind || kind === "custom") {
    return "AskUserQuestion";
  }
  return kind;
}
function notify(hookEventName, ctx, toolName) {
  if (isNestedOmpAgent(ctx)) {
    return Promise.resolve();
  }

  return new Promise((resolve) => {
    const payload = {
      unpeel_runtime_id: runtimeId,
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
  if (runtimeId === "omp") {
    extension.on("agent_start", async (_event, ctx) => {
      await notify("Start", ctx);
    });
    extension.on("agent_end", async (event, ctx) => {
      if (event?.willContinue || event?.isTerminal === false) {
        return;
      }
      await notify("Stop", ctx);
    });
    extension.on("tool_call", async (event, ctx) => {
      if (event?.toolName === "ask") {
        await notify("PermissionRequest", ctx, "ask");
      }
    });
    extension.on("tool_result", async (event, ctx) => {
      if (event?.toolName === "ask") {
        await notify("UserPromptSubmit", ctx);
      }
    });
    extension.on("tool_approval_requested", async (event, ctx) => {
      const toolName =
        typeof event?.toolName === "string" && event.toolName
          ? event.toolName
          : "tool_approval";
      await notify("PermissionRequest", ctx, toolName);
    });
    extension.on("tool_approval_resolved", async (_event, ctx) => {
      await notify("UserPromptSubmit", ctx);
    });
  } else if (runtimeId === "pi") {
    extension.on("agent_start", async (_event, ctx) => {
      await notify("Start", ctx);
    });
    extension.on("agent_settled", async (_event, ctx) => {
      await notify("Stop", ctx);
    });
    extension.on("session_shutdown", async (event, ctx) => {
      if (event?.reason === "quit") {
        await notify("Stop", ctx);
      }
    });
    extension.on("ui_prompt_start", async (event, ctx) => {
      await notify("PermissionRequest", ctx, promptToolName(event));
    });
    extension.on("ui_prompt_end", async (_event, ctx) => {
      await notify("UserPromptSubmit", ctx);
    });
  }
}
