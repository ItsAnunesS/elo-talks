import { invoke } from "./bridge.js";
import { showLoading, hideLoading } from "./loading-screen.js";
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

export async function prepareAccounts(accounts, lastAccount, openAccount, waitForAccount = true) {
  const priority = accounts.find((account) => account.id === lastAccount) || accounts[0];
  if (!priority) {
    hideLoading();
    return;
  }
  const proceed = document.getElementById("startup-continue");
  let skipped = !waitForAccount;
  let resolveContinue;
  const continued = new Promise((resolve) => {
    resolveContinue = resolve;
  });
  const skip = () => {
    skipped = true;
    resolveContinue();
  };
  proceed.addEventListener("click", skip);
  showLoading(`Opening ${priority.name}…`);
  try {
    await invoke("sync_accounts", { accountId: priority.id });
    // Give the priority view a head start. Other views stay hidden and are
    // created incrementally; their readiness never blocks the selected account.
    void (async () => {
      let failed = 0;
      for (const account of accounts.filter((account) => account.id !== priority.id)) {
        await pause(500);
        try {
          await invoke("sync_accounts", { accountId: account.id });
        } catch {
          failed++;
        }
      }
      if (failed)
        window.dispatchEvent(
          new CustomEvent("ui-error", {
            detail: `${failed} background ${failed === 1 ? "account could" : "accounts could"} not start. Select a channel to retry.`,
          }),
        );
    })();
    const started = Date.now();
    while (!skipped) {
      const snapshot = await invoke("account_snapshot");
      const health = snapshot.accounts?.[priority.id];
      if (["chats", "qr-code"].includes(health?.readiness)) break;
      const elapsed = Date.now() - started;
      const failed = health?.page && !["loading", "ready"].includes(health.page);
      if (!failed && elapsed >= 5000 && elapsed < 40000) {
        showLoading(
          `Still opening ${priority.name}. Other accounts are preparing in the background…`,
        );
      }
      proceed.hidden = elapsed < 5000 && !failed;
      if (failed || elapsed >= 40000) {
        showLoading(
          failed
            ? "WhatsApp could not load. Continue to retry the account."
            : "WhatsApp is taking longer than usual. You can continue while it finishes.",
        );
        // End snapshot polling after the bounded readiness probe expires.
        await continued;
        break;
      }
      await pause(400);
    }
    await openAccount(priority.id);
  } finally {
    proceed.hidden = true;
    proceed.removeEventListener("click", skip);
    hideLoading();
  }
}
