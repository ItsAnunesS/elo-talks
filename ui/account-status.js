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

let organization = { placements: {} };
let activeWorkspace;
function setBadge(element, count) {
  let badge = element.querySelector(":scope > .notification-badge");
  if (!badge) {
    badge = createElement("span", "notification-badge");
    element.append(badge);
  }
  badge.hidden = count === 0;
  badge.textContent = count > 99 ? "99+" : String(count);
  badge.title = `${count} notifications since last viewed`;
  badge.setAttribute("aria-label", badge.title);
}
export function updateAccountStatus(next, selected, groups, workspaceId) {
  if (next) snapshot = next;
  if (groups) {
    organization = groups;
    activeWorkspace = workspaceId;
  }
  const totals = new Map();
  for (const [id, placement] of Object.entries(organization.placements)) {
    totals.set(
      placement.workspaceId,
      (totals.get(placement.workspaceId) || 0) + (snapshot.accounts[id]?.pendingNotifications || 0),
    );
  }
  for (const element of getElement("workspace-icons").children)
    setBadge(element, totals.get(element.dataset.workspaceId) || 0);
  for (const element of document.querySelectorAll("#account-list [data-account-id]")) {
    const health = snapshot.accounts[element.dataset.accountId];
    setBadge(element, health?.pendingNotifications || 0);
    let status = element.querySelector(".account-health");
    if (!status) {
      status = createElement("small", "account-health");
      element.append(status);
    }
    status.textContent = labels[health?.page] || "Opening account…";
    status.title = status.textContent;
    status.hidden = health?.page === "ready";
    element.querySelector(".account-name").title = status.textContent;
  }
  const total = totals.get(activeWorkspace) || 0;
  getElement("overview-badge").hidden = total === 0;
  getElement("overview-badge").textContent = total > 99 ? "99+" : String(total);
  const health = snapshot.accounts[selected];
  getElement("account-page-status").textContent = selected
    ? health?.page === "ready"
      ? "WhatsApp Web"
      : labels[health?.page] || "Opening account…"
    : "Vault open";
  getElement("retry-account").hidden = !selected || health?.page === "sandbox-unavailable";
}
