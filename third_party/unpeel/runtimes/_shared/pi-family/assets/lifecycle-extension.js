import { spawn } from "node:child_process";

const notifyPath = {{NOTIFY_PATH_JSON}};

function notify(hookEventName) {
  return new Promise((resolve) => {
    const child = spawn(
      "bash",
      [notifyPath, JSON.stringify({ hook_event_name: hookEventName })],
      { stdio: "ignore" },
    );
    child.once("error", resolve);
    child.once("exit", resolve);
  });
}

export default function registerUnpeelLifecycle(extension) {
  extension.on("agent_start", async () => {
    await notify("Start");
  });
  extension.on("agent_end", async () => {
    await notify("Stop");
  });
}
