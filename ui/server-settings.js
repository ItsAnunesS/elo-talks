import {
  createElement,
  createField,
  createButton,
  createDestructiveAction,
  errorMessage,
} from "./dom.js";
import { request } from "./bridge.js";

export function serverSettings(parent, { status, refresh, feedback, signal }) {
  let pollTimer, lockTimer;
  const section = createElement("section", "dialog-section");
  section.append(
    createElement("h3", "", "Your server"),
    createElement(
      "p",
      "",
      "Connect to your organization's HTTPS server to synchronize the workspaces you can access. WhatsApp sessions stay on this computer and may need linking on each device.",
    ),
  );
  const state = createElement("p", "hint");
  state.setAttribute("role", "status");
  state.setAttribute("aria-live", "polite");
  section.append(state);
  const domain = createField(
    section,
    "Server domain",
    "text",
    status?.domain || "",
    "panel.example.com",
  );
  domain.maxLength = 2048;
  const username = createField(section, "Server username", "text", status?.username || "");
  username.maxLength = 64;
  const password = createField(section, "Server password", "password");
  password.maxLength = 1024;
  const advanced = createElement("details"),
    summary = createElement("summary", "", "Private certificate (optional)");
  advanced.append(summary);
  const certificateLabel = createElement("label", "", "Trusted PEM certificate"),
    certificate = createElement("textarea");
  certificate.id = `server-certificate-${crypto.randomUUID()}`;
  certificateLabel.htmlFor = certificate.id;
  certificate.rows = 4;
  certificate.maxLength = 65536;
  advanced.append(
    certificateLabel,
    certificate,
    createElement(
      "p",
      "hint",
      "For a private or localhost certificate, obtain the PEM from your administrator through a trusted channel. Certificate validation always stays enabled.",
    ),
  );
  section.append(advanced);
  const actions = createElement("div", "dialog-actions");
  section.append(actions);
  let busy = false;
  async function watch() {
    if (signal.aborted) return;
    const current = await request("server.status");
    render(current);
    if (["syncing", "checking"].includes(current.phase)) {
      pollTimer = setTimeout(() => watch().catch((error) => feedback(errorMessage(error))), 750);
      return;
    }
    await refresh();
  }
  const connect = createButton("Connect and sign in", async () => {
    if (!domain.value.trim() || !username.value.trim() || !password.value)
      throw Error("Enter the server domain, username and password.");
    busy = true;
    connect.disabled = true;
    try {
      const current = await request("server.connect", {
        domain: domain.value,
        username: username.value,
        password: password.value,
        certificate: certificate.value,
      });
      render(current);
      clearTimeout(pollTimer);
      pollTimer = setTimeout(() => watch().catch((error) => feedback(errorMessage(error))), 750);
    } finally {
      password.value = "";
      busy = false;
    }
  });
  actions.append(connect);
  const organizationLabel = createElement("label", "", "Organization"),
    organization = createElement("select");
  organization.id = `server-organization-${crypto.randomUUID()}`;
  organizationLabel.htmlFor = organization.id;
  section.append(organizationLabel, organization);
  const select = createButton("Use organization", async () => {
    if (!organization.value) throw Error("Select an organization first.");
    render(await request("server.select", { organizationId: organization.value }));
    clearTimeout(pollTimer);
    pollTimer = setTimeout(() => watch().catch((error) => feedback(errorMessage(error))), 750);
  });
  const sync = createButton("Sync now", async () => {
    render(await request("server.sync"));
    clearTimeout(pollTimer);
    pollTimer = setTimeout(() => watch().catch((error) => feedback(errorMessage(error))), 750);
  });
  const management = createElement("p", "hint");
  management.append(
    document.createTextNode(
      "Manage users, roles and workspace structure in your server panel. Server workspaces are read-only in this app.",
    ),
  );
  section.append(createElement("div", "dialog-actions"), management);
  section.lastElementChild.previousElementSibling.append(select, sync);
  const disconnect = createDestructiveAction(
    "Disconnect server",
    "Disconnect and remove managed channels and their local profiles from this computer? Local workspaces remain available.",
    async () => {
      await request("server.disconnect");
      await refresh();
      feedback("Server disconnected.");
    },
  );
  section.append(disconnect);
  function render(value = {}) {
    const processing = ["syncing", "checking"].includes(value.phase);
    const connected = value.phase === "connected";
    state.textContent =
      value.error ||
      (processing
        ? "Verifying server access… Local workspaces stay available."
        : connected
          ? !value.organizations?.length
            ? "Signed in. No organizations are assigned; ask your server administrator."
            : !value.organizationId
              ? "Signed in. Choose an organization to synchronize."
              : "Connected. Access and workspace changes are checked every 30 seconds."
          : "Local mode. Your existing workspaces stay on this computer.");
    connect.disabled = busy || processing;
    select.disabled = processing || !value.organizations?.length;
    sync.disabled = processing || !value.domain;
    disconnect.hidden = !value.domain;
    clearTimeout(lockTimer);
    if (processing)
      lockTimer = setTimeout(() => {
        if (!signal.aborted) {
          connect.disabled = true;
          select.disabled = true;
          sync.disabled = true;
        }
      }, 0);
    organization.replaceChildren(
      ...(value.organizations || []).map((item) => {
        const option = createElement(
          "option",
          "",
          `${item.name} · ${item.role.replaceAll("_", " ")}`,
        );
        option.value = item.id;
        return option;
      }),
    );
    organization.value = value.organizationId || "";
  }
  render(status);
  signal.addEventListener(
    "abort",
    () => {
      clearTimeout(pollTimer);
      clearTimeout(lockTimer);
      password.value = "";
      certificate.value = "";
    },
    { once: true },
  );
  parent.append(section);
  if (["syncing", "checking"].includes(status?.phase))
    pollTimer = setTimeout(() => watch().catch((error) => feedback(errorMessage(error))), 750);
}
