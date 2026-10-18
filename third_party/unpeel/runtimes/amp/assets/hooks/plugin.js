// @i-know-the-amp-plugin-api-is-wip-and-very-experimental-right-now
const notifyPath = "{{NOTIFY_PATH}}";

async function notify(hookEventName, payload = {}) {
  const body = JSON.stringify({
    unpeel_runtime_id: "amp",
    hook_event_name: hookEventName,
    ...payload,
  });

  try {
    const proc = Bun.spawn(["bash", notifyPath, body], {
      stdin: "ignore",
      stdout: "ignore",
      stderr: "ignore",
    });
    await proc.exited;
  } catch {
    // Best effort.
  }
}

function threadIDFrom(event) {
  const candidates = [
    event?.threadID,
    event?.threadId,
    event?.thread?.id,
    event?.conversationID,
    event?.conversationId,
  ];
  return candidates.find((value) => typeof value === "string" && value.length > 0);
}

export default function registerUnpeelAmpPlugin(amp) {
  let rootThreadID = null;
  const pendingQuestions = new Set();

  amp.on("agent.start", async (event) => {
    const threadID = threadIDFrom(event);
    if (!rootThreadID && threadID) rootThreadID = threadID;
    if (threadID && rootThreadID && threadID !== rootThreadID) return;
    pendingQuestions.clear();
    await notify("Start", {
      session_id: threadID || rootThreadID,
    });
  });

  amp.on("tool.call", async (event) => {
    const threadID = threadIDFrom(event);
    if (threadID && rootThreadID && threadID !== rootThreadID) return { action: "allow" };
    if (event?.tool === "ask_user_choice") {
      const id = event?.toolUseID;
      if (id) pendingQuestions.add(id);
      await notify("PermissionRequest", {
        session_id: threadID || rootThreadID,
        tool_name: "ask_user_choice",
      });
    }
    return { action: "allow" };
  });

  amp.on("tool.result", async (event) => {
    const threadID = threadIDFrom(event);
    if (threadID && rootThreadID && threadID !== rootThreadID) return;
    if (event?.tool === "ask_user_choice") {
      const id = event?.toolUseID;
      const removed = id ? pendingQuestions.delete(id) : false;
      if (removed && pendingQuestions.size === 0) {
        await notify("UserPromptSubmit", {
          session_id: threadID || rootThreadID,
        });
      }
    }
  });

  amp.on("agent.end", async (event) => {
    const threadID = threadIDFrom(event);
    if (threadID && rootThreadID && threadID !== rootThreadID) return;
    pendingQuestions.clear();
    await notify("Stop", {
      session_id: threadID || rootThreadID,
    });
    rootThreadID = null;
  });
}
