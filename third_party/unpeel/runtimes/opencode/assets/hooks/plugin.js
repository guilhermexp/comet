// Unpeel OpenCode plugin
export const UnpeelNotifyPlugin = async ({ $, client }) => {
  if (globalThis.__unpeelOpencodeNotifyPluginV1) return {};
  globalThis.__unpeelOpencodeNotifyPluginV1 = true;

  if (!process?.env?.UNPEEL_SESSION_ID) return {};

  const notifyPath = "{{NOTIFY_PATH}}";
  let currentState = 'idle';
  let rootSessionID = null;
  let stopSent = false;
  const childSessionCache = new Map();
  const pendingPermissions = new Set();
  const pendingQuestions = new Set();

  const notify = async (hookEventName, sessionID = rootSessionID, toolName) => {
    const payload = JSON.stringify({
      unpeel_runtime_id: "opencode",
      hook_event_name: hookEventName,
      ...(sessionID ? { session_id: sessionID } : {}),
      ...(toolName ? { tool_name: toolName } : {}),
    });
    try {
      await $`bash ${notifyPath} ${payload}`;
    } catch {
      // Best effort.
    }
  };

  const isChildSession = async (sessionID) => {
    if (!sessionID || !client?.session?.list) return true;
    if (childSessionCache.has(sessionID)) {
      return childSessionCache.get(sessionID);
    }
    try {
      const sessions = await client.session.list();
      const session = sessions.data?.find((value) => value.id === sessionID);
      const isChild = !!session?.parentID;
      childSessionCache.set(sessionID, isChild);
      return isChild;
    } catch {
      return true;
    }
  };

  const handleBusy = async (sessionID) => {
    if (!rootSessionID) rootSessionID = sessionID;
    if (sessionID !== rootSessionID) return;
    if (currentState === 'idle') {
      currentState = 'busy';
      stopSent = false;
      pendingPermissions.clear();
      pendingQuestions.clear();
      await notify('Start', sessionID);
    }
  };

  const handleStop = async (sessionID) => {
    if (rootSessionID && sessionID !== rootSessionID) return;
    if (currentState === 'busy' && !stopSent) {
      currentState = 'idle';
      stopSent = true;
      pendingPermissions.clear();
      pendingQuestions.clear();
      await notify('Stop', sessionID);
      rootSessionID = null;
    }
  };

  return {
    event: async ({ event }) => {
      const sessionID = event.properties?.sessionID;
      if (await isChildSession(sessionID)) return;

      if (event.type === 'session.status') {
        const status = event.properties?.status;
        if (status?.type === 'busy') {
          await handleBusy(sessionID);
        } else if (status?.type === 'idle') {
          await handleStop(sessionID);
        }
      }

      if (event.type === 'session.busy') {
        await handleBusy(sessionID);
      }
      if (event.type === 'session.idle' || event.type === 'session.error') {
        await handleStop(sessionID);
      }
      if (!rootSessionID) rootSessionID = sessionID;
      if (sessionID !== rootSessionID) return;

      if (event.type === 'permission.asked' || event.type === 'permission.v2.asked') {
        const id =
          event.properties?.id || event.properties?.requestID || event.id || event.requestID;
        if (id) pendingPermissions.add(id);
        const toolName =
          event.properties?.action ||
          event.properties?.tool?.name ||
          event.properties?.permission ||
          "permission";
        await notify('PermissionRequest', sessionID, toolName);
      }
      if (event.type === 'permission.replied' || event.type === 'permission.v2.replied') {
        const id =
          event.properties?.requestID || event.properties?.id || event.requestID || event.id;
        const removed = id ? pendingPermissions.delete(id) : false;
        if (removed && pendingPermissions.size === 0 && pendingQuestions.size === 0) {
          await notify('UserPromptSubmit', sessionID);
        }
      }

      if (event.type === 'question.asked' || event.type === 'question.v2.asked') {
        const id =
          event.properties?.id || event.properties?.requestID || event.id || event.requestID;
        if (id) pendingQuestions.add(id);
        const toolName =
          event.properties?.tool?.name || "question";
        await notify('PermissionRequest', sessionID, toolName);
      }
      if (
        event.type === 'question.replied' ||
        event.type === 'question.v2.replied' ||
        event.type === 'question.rejected' ||
        event.type === 'question.v2.rejected'
      ) {
        const id =
          event.properties?.requestID || event.properties?.id || event.requestID || event.id;
        const removed = id ? pendingQuestions.delete(id) : false;
        if (removed && pendingPermissions.size === 0 && pendingQuestions.size === 0) {
          await notify('UserPromptSubmit', sessionID);
        }
      }
    },
  };
};
