import { createElement, getElement } from "./dom.js";

const labels = {
  loading: "Loading WhatsApp…",
  ready: "Page loaded · sign in if needed",
  "load-failed": "Could not load · check your connection",
  "tls-failed": "Secure connection failed",
  "process-stopped": "Account stopped unexpectedly",
  "sandbox-unavailable": "Security sandbox unavailable",
};
let snapshot = { accounts: {}, preferences: {} };

export function updateAccountStatus(next, selected) {
  if (next) snapshot = next;
  let total = 0;
  for (const container of ["account-icons", "account-list"]) {
    for (const element of getElement(container).children) {
      const health = snapshot.accounts[element.dataset.accountId];
      const count = health?.pendingNotifications || 0;
      let badge = element.querySelector(".notification-badge");
      if (!badge) {
        badge = createElement("span", "notification-badge");
        element.append(badge);
      }
      badge.hidden = count === 0;
      badge.textContent = count > 99 ? "99+" : String(count);
      badge.title = `${count} notifications since last viewed`;
      badge.setAttribute("aria-label", badge.title);
      if (container === "account-list") {
        total += count;
        let status = element.querySelector(".account-health");
        if (!status) {
          status = createElement("small", "account-health");
          element.append(status);
        }
        status.textContent = labels[health?.page] || "Opening account…";
        status.title = status.textContent;
      }
    }
  }
  getElement("overview-badge").hidden = total === 0;
  getElement("overview-badge").textContent = total > 99 ? "99+" : String(total);
  const health = snapshot.accounts[selected];
  getElement("account-page-status").textContent = selected
    ? labels[health?.page] || "Opening account…"
    : "Vault open";
  getElement("retry-account").hidden = !selected || health?.page === "sandbox-unavailable";
}
