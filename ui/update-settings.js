import { createElement, createButton } from "./dom.js";
import { request } from "./bridge.js";

export function updateSettings(parent, { preferences, version, restart, feedback, signal }) {
  const heading = createElement("div", "settings-version");
  heading.append(
    createElement("h3", "", `Elo Talks ${version || "development"}`),
    createElement("span", "release-channel", "Stable channel"),
  );
  parent.append(
    heading,
    createElement("p", "hint", "Updates are installed automatically when the app starts."),
  );
  const status = createElement("p", "update-status", "Checking update status…");
  status.setAttribute("role", "status");
  const check = createElement("button", "secondary", "Check now");
  check.type = "button";
  check.addEventListener("click", async () => {
    if (check.disabled) return;
    check.disabled = true;
    check.setAttribute("aria-busy", "true");
    try {
      await request("update.check");
      await refresh();
    } catch {
      status.textContent = "Could not check now. Wait for the next scheduled attempt.";
      await refresh().catch(() => {});
    } finally {
      check.removeAttribute("aria-busy");
    }
  });
  const install = createButton("Restart to update", restart, "primary");
  install.hidden = true;
  const actions = createElement("div", "dialog-actions");
  actions.append(check, install);
  parent.append(status, actions);
  function display(next) {
    const wait = Number(next.retryInSeconds) || 0;
    const labels = {
      idle: "Ready to check for updates.",
      checking: "Checking GitHub for a newer version…",
      current: "You’re up to date.",
      available: `Version ${next.version || ""} is available.`,
      ready: `Version ${next.version || ""} is ready to install.`,
      unsupported: "Automatic updates are available in the installed AppImage.",
      error: "The last update check failed. You can keep using the app.",
      downloading: "Downloading and verifying the update…",
      installing: "Installing the verified update…",
    };
    status.textContent =
      (labels[next.phase] || "Update status unavailable.") +
      (wait ? ` Next check available in ${Math.ceil(wait / 60)} min.` : "");
    check.disabled =
      next.canCheck === false ||
      ["checking", "downloading", "installing", "unsupported"].includes(next.phase);
    install.hidden = !["available", "ready"].includes(next.phase);
  }
  async function refresh() {
    display(await request("update.status"));
  }
  window.addEventListener("update-state", (event) => display(event.detail), { signal });
  void refresh().catch(() => {
    status.textContent = "Update status unavailable. Try again shortly.";
  });
  const section = createElement("section", "dialog-section");
  const label = createElement("label", "", "Check for updates every");
  const interval = createElement("select");
  interval.id = `update-interval-${crypto.randomUUID()}`;
  label.htmlFor = interval.id;
  for (const minutes of [5, 10, 15, 30, 60]) {
    const option = createElement("option", "", `${minutes} minutes`);
    option.value = String(minutes);
    interval.append(option);
  }
  interval.value = String(preferences?.intervalMinutes || 5);
  section.append(
    label,
    interval,
    createElement(
      "p",
      "hint",
      "Updates install before unlocking at startup. During use, you decide when to restart. GitHub limits and errors can extend this interval.",
    ),
    createButton("Save update interval", async () => {
      await request("update.configure", { intervalMinutes: Number(interval.value) });
      feedback("Update interval saved.");
    }),
  );
  parent.append(section);
}
