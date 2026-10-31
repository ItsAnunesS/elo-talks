import { icon } from "./icons.js";
import { createElement, getElement } from "./dom.js";

const labels = {
  loading: "Loading WhatsApp…",
  ready: "Page loaded · sign in if needed",
  "load-failed": "Could not load · check your connection",
  "tls-failed": "Secure connection failed",
  "process-stopped": "Account stopped unexpectedly",
  "sandbox-unavailable": "Security sandbox unavailable",
};
function healthLabel(health) {
  if (health?.page !== "ready") return labels[health?.page] || "Preparing WhatsApp…";
  if (health.readiness === "qr-code") return "Scan QR code to connect";
  if (health.readiness === "chats") return "Open WhatsApp →";
  return "Open WhatsApp · sign in if needed";
}
function needsAttention(health) {
  return !!health?.page && !["loading", "ready"].includes(health.page);
}
let snapshot = { accounts: {}, preferences: {} };

let organization = { placements: {} };
let activeWorkspace;
function counter(health) {
  return health?.unreadCount ?? health?.pendingNotifications ?? 0;
}
function setBadge(element, count, source = "Unread items / pending notifications") {
  let badge = element.querySelector(":scope > .notification-badge");
  if (!badge) {
    badge = createElement("span", "notification-badge");
    element.append(badge);
  }
  badge.hidden = count === 0;
  badge.textContent = count > 99 ? "99+" : String(count);
  badge.title = `${count} · ${source}`;
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
      (totals.get(placement.workspaceId) || 0) + counter(snapshot.accounts[id]),
    );
  }
  for (const element of getElement("workspace-icons").children)
    setBadge(element, totals.get(element.dataset.workspaceId) || 0);
  for (const element of document.querySelectorAll("#account-list [data-account-id]")) {
    const health = snapshot.accounts[element.dataset.accountId];
    setBadge(
      element,
      counter(health),
      health?.unreadCount != null
        ? "Unread counter reported by WhatsApp"
        : "Notifications since last viewed",
    );
    let status = element.querySelector(".account-health");
    if (!status) {
      status = createElement("small", "account-health");
      element.append(status);
    }
    const muted = snapshot.preferences?.mutedAccounts?.includes(element.dataset.accountId);
    let indicator = element.querySelector(".channel-muted");
    if (!indicator) {
      indicator = createElement("span", "channel-muted");
      indicator.append(icon("bellOff"));
      element.querySelector(".account-name").append(indicator);
    }
    indicator.hidden = !muted;
    indicator.title = "Notifications muted";
    indicator.setAttribute("aria-label", "Notifications muted");
    status.textContent = healthLabel(health);
    status.title = status.textContent;
    status.hidden = health?.page === "ready" && health?.readiness !== "qr-code";
    element.querySelector(".account-name").title = status.textContent;
  }
  let attention = 0,
    preparing = 0;
  for (const card of document.querySelectorAll("#cards [data-account-id]")) {
    const health = snapshot.accounts[card.dataset.accountId];
    const failed = needsAttention(health);
    card.classList.toggle("needs-attention", failed);
    card.querySelector(".card-health").textContent = healthLabel(health);
    card.title = healthLabel(health);
    setBadge(card, counter(health));
    if (failed) attention++;
    else if (!health || health.page === "loading") preparing++;
  }
  const summary = getElement("workspace-health");
  summary.textContent = attention
    ? `${attention} ${attention === 1 ? "channel needs" : "channels need"} attention.`
    : preparing
      ? `${preparing} ${preparing === 1 ? "channel is" : "channels are"} loading…`
      : "";
  summary.classList.toggle("has-warning", attention > 0);
  const total = totals.get(activeWorkspace) || 0;
  getElement("overview-badge").hidden = total === 0;
  getElement("overview-badge").textContent = total > 99 ? "99+" : String(total);
  const health = snapshot.accounts[selected];
  getElement("account-page-status").textContent = selected
    ? health?.page === "ready"
      ? health?.readiness === "qr-code"
        ? "Scan QR code"
        : health?.readiness === "chats"
          ? ""
          : "Opening WhatsApp…"
      : labels[health?.page] || "Opening account…"
    : "Vault open";
  const retry = getElement("retry-account");
  retry.hidden = !selected || !needsAttention(health) || health?.page === "sandbox-unavailable";
  retry.textContent = health?.page === "process-stopped" ? "Reconnect" : "Try again";
  getElement("account-page-status").title =
    health?.readiness === "qr-code"
      ? "On your phone, open WhatsApp → Linked devices → Link a device."
      : health?.page === "tls-failed"
        ? "Check your connection and system clock. Certificate validation remains enabled."
        : "";
}
