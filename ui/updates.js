import { showLoading as show, hideLoading, showUpdate } from "./loading-screen.js";
import { request, invoke } from "./bridge.js";
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const button = document.getElementById("update-now");
let busy = false;
async function settled(timeout = 320000, progress = false) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    const status = await request("update.status");
    if (progress) showUpdate(status);
    if (!["checking", "downloading", "installing"].includes(status.phase)) return status;
    await pause(600);
  }
  throw new Error("The update took too long. Your current installation is still available.");
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
    if (["checking", "downloading"].includes(status.phase)) status = await settled(320000, true);
    if (status.phase === "available") {
      await request("update.prepare");
      status = await settled(320000, true);
    }
    if (status.phase !== "ready")
      throw new Error(status.error || "Update unavailable. Try again later.");
    showUpdate({ ...status, phase: "installing" });
    await request("update.install");
  } catch (error) {
    hideLoading();
    window.dispatchEvent(new CustomEvent("ui-error", { detail: error.message }));
    busy = false;
    button.disabled = false;
    window.dispatchEvent(new CustomEvent("update-failed"));
  }
}
// Install the verified artifact before restarting; reopening the old image alone
// cannot guarantee an update, especially when the next startup check is offline.
export async function restartForUpdate() {
  await install();
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
    const result = await settled(320000, true);
    if (result.phase === "installed") return false;
    hideLoading();
    window.dispatchEvent(
      new CustomEvent("ui-error", {
        detail: result.error || "Update failed. You can unlock and continue.",
      }),
    );
  }
  if (startup.automatic && !unlocked && !current.restartMismatch) {
    show("Checking for updates…");
    try {
      const initial = await request("update.status");
      if (initial.phase !== "unsupported") {
        if (!["available", "ready", "checking", "downloading"].includes(initial.phase)) {
          await request("update.check");
        }
        const status = await settled(320000, true);
        if (["available", "ready"].includes(status.phase)) await install();
        else if (status.phase === "error") throw new Error(status.error || "Update check failed.");
      }
    } catch (error) {
      window.dispatchEvent(
        new CustomEvent("ui-error", {
          detail: `${error.message || "Update check failed."} You can continue using the app.`,
        }),
      );
    }
    if (!busy) hideLoading();
  }
  button.addEventListener("click", restartForUpdate);
  setInterval(async () => {
    if (busy) return;
    try {
      const status = await request("update.status");
      window.dispatchEvent(new CustomEvent("update-state", { detail: status }));
      button.hidden = !["available", "ready", "downloading"].includes(status.phase);
      button.disabled = status.phase !== "ready";
      button.textContent =
        status.phase === "ready"
          ? `Restart to update${status.version ? ` · ${status.version}` : ""} · Ready`
          : status.progress?.step === "downloading" && status.progress.total > 0
            ? `Downloading update · ${Math.floor((status.progress.downloaded / status.progress.total) * 100)}%`
            : "Preparing update…";
    } catch {
      /* The controller may be restarting the interface. */
    }
  }, 10000);
  return !busy;
}
