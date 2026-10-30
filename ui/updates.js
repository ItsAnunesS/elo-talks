import { showLoading as show, hideLoading } from "./loading-screen.js";
import { request, invoke } from "./bridge.js";
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const button = document.getElementById("update-now");
let busy = false;
async function settled() {
  for (;;) {
    const status = await request("update.status");
    if (!["checking", "downloading", "installing"].includes(status.phase)) return status;
    await pause(600);
  }
}
async function install() {
  if (busy) return;
  busy = true;
  button.disabled = true;
  try {
    // Hide native account views before displaying local update UI.
    await invoke("select_account", { id: null });
    show("Downloading and verifying the update…");
    let status = await request("update.status");
    if (status.phase === "available") {
      await request("update.prepare");
      status = await settled();
    }
    if (status.phase !== "ready")
      throw new Error(status.error || "Update unavailable. Try again later.");
    show("Closing your vault and restarting…");
    await request("update.install");
  } catch (error) {
    hideLoading();
    window.dispatchEvent(new CustomEvent("ui-error", { detail: error.message }));
    busy = false;
    button.disabled = false;
  }
}
export async function initializeUpdates(unlocked) {
  const startup = await request("update.startup");
  const current = await request("update.status");
  if (current.phase === "error")
    window.dispatchEvent(
      new CustomEvent("ui-error", {
        detail: current.error || "Update unavailable. You can continue using the app.",
      }),
    );
  if (current.phase === "installing") {
    show("Installing the verified update. Your vault is closed…");
    const result = await settled();
    if (result.phase === "installed") return false;
    hideLoading();
    window.dispatchEvent(
      new CustomEvent("ui-error", {
        detail: result.error || "Update failed. You can unlock and continue.",
      }),
    );
  }
  if (startup.automatic && !unlocked) {
    show("Checking for updates…");
    try {
      const initial = await request("update.status");
      if (initial.phase !== "unsupported") {
        await request("update.check");
        const status = await settled();
        if (status.phase === "available") await install();
      }
    } catch {
      /* Offline startup must still allow vault access. */
    }
    if (!busy) hideLoading();
  }
  button.addEventListener("click", install);
  setInterval(async () => {
    if (busy) return;
    try {
      const status = await request("update.status");
      button.hidden = !["available", "ready"].includes(status.phase);
      button.textContent = `Update now${status.version ? ` · ${status.version}` : ""}`;
    } catch {
      /* The controller may be restarting the interface. */
    }
  }, 10000);
  return !busy;
}
