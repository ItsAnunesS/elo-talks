import { createButton, createElement, getElement } from "./dom.js";
import { request } from "./bridge.js";

const images = new Map();
const collapsed = new Set();
let generation = 0;
export function clearWorkspaceImages() {
  generation++;
  images.clear();
  collapsed.clear();
  getElement("workspace-icons").replaceChildren();
}
export function serviceIcon(service = "whatsapp") {
  const icon = createElement("img", "service-icon");
  icon.src = `assets/${service === "whatsapp" ? "whatsapp" : "elo-mark"}.svg`;
  icon.alt = "";
  icon.setAttribute("aria-hidden", "true");
  return icon;
}
async function loadImage(workspace, element) {
  if (!workspace.imageRevision) return;
  const revision = workspace.imageRevision;
  const captured = generation;
  if (!images.has(revision)) images.set(revision, request("workspace.image", { id: workspace.id }));
  try {
    const source = await images.get(revision);
    if (
      captured !== generation ||
      !element.isConnected ||
      !source?.startsWith("data:image/png;base64,")
    )
      return;
    const image = createElement("img", "workspace-image");
    image.alt = "";
    image.src = source;
    element.replaceChildren(image);
  } catch {
    images.delete(revision);
    // An unavailable custom icon retains the workspace initials.
  }
}
export function renderNavigation(state, workspaceId, selected, actions) {
  const { workspaces, placements } = state.organization;
  const workspace = workspaces.find((w) => w.id === workspaceId);
  const revisions = new Set(workspaces.map((w) => w.imageRevision));
  for (const revision of images.keys()) if (!revisions.has(revision)) images.delete(revision);
  getElement("workspace-icons").replaceChildren();
  for (const item of workspaces) {
    const button = createButton(
      "",
      () => actions.selectWorkspace(item.id),
      "account-icon workspace-icon",
    );
    button.dataset.workspaceId = item.id;
    button.title = item.name;
    button.setAttribute("aria-label", item.name);
    button.setAttribute("aria-pressed", String(item.id === workspaceId));
    button.classList.toggle("selected", item.id === workspaceId);
    const avatar = createElement("span", "workspace-avatar", item.name.slice(0, 2).toUpperCase());
    button.append(avatar);
    getElement("workspace-icons").append(button);
    void loadImage(item, avatar);
  }
  getElement("workspace-name").textContent = workspace.name;
  getElement("workspace-breadcrumb").textContent = workspace.name;
  getElement("dashboard-title").textContent = workspace.name;
  const accounts = state.accounts.filter((a) => placements[a.id]?.workspaceId === workspaceId);
  getElement("account-count").textContent = accounts.length;
  getElement("account-list").replaceChildren();
  getElement("cards").replaceChildren();
  const appendChannels = (parent, categoryId) => {
    for (const account of accounts.filter((a) => placements[a.id].categoryId === categoryId)) {
      const row = createElement("div", "account-row");
      row.dataset.accountId = account.id;
      row.classList.toggle("selected", selected === account.id);
      const button = createButton("", () => actions.selectAccount(account.id), "account-name");
      button.setAttribute("aria-label", account.name);
      button.append(
        serviceIcon(placements[account.id].service),
        createElement("span", "channel-label", account.name),
      );
      const menu = createButton("⋯", () => actions.editAccount(account), "account-menu");
      menu.setAttribute("aria-label", `Manage ${account.name}`);
      row.append(button, menu);
      parent.append(row);
    }
  };
  appendChannels(getElement("account-list"), null);
  for (const category of workspace.categories) {
    const section = createElement("section", "channel-category");
    const heading = createElement("div", "category-heading");
    const channels = createElement("div", "category-channels");
    channels.hidden = collapsed.has(category.id);
    const toggle = createButton(
      "",
      () => {
        channels.hidden = !channels.hidden;
        if (channels.hidden) collapsed.add(category.id);
        else collapsed.delete(category.id);
        toggle.setAttribute("aria-expanded", String(!channels.hidden));
        caret.textContent = channels.hidden ? "▸" : "▾";
      },
      "category-toggle",
    );
    toggle.setAttribute("aria-expanded", String(!channels.hidden));
    const caret = createElement("span", "", channels.hidden ? "▸" : "▾");
    toggle.append(caret, createElement("span", "channel-label", category.name));
    const edit = createButton("⋯", () => actions.editCategory(category), "account-menu");
    edit.setAttribute("aria-label", `Manage category ${category.name}`);
    heading.append(toggle, edit);
    appendChannels(channels, category.id);
    if (!channels.childElementCount)
      channels.append(createElement("p", "empty-category", "No channels yet"));
    section.append(heading, channels);
    getElement("account-list").append(section);
  }
  for (const account of accounts) {
    const card = createButton("", () => actions.selectAccount(account.id), "account-card");
    const avatar = createElement(
      "span",
      "card-avatar account-avatar",
      account.name.slice(0, 2).toUpperCase(),
    );
    avatar.dataset.avatarAccount = account.id;
    const title = createElement("strong", "channel-card-title");
    title.append(serviceIcon(), createElement("span", "", account.name));
    card.append(avatar, title, createElement("small", "", "Open WhatsApp →"));
    getElement("cards").append(card);
  }
}

export function revealChannel(id) {
  for (const row of document.querySelectorAll("#account-list [data-account-id]")) {
    if (row.dataset.accountId !== id) continue;
    const section = row.closest(".channel-category");
    if (!section) return;
    const toggle = section.querySelector(".category-toggle");
    if (toggle.getAttribute("aria-expanded") === "false") toggle.click();
    row.scrollIntoView({ block: "nearest" });
  }
}
