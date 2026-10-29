import { createButton, createElement, createField } from "./dom.js";
import { serviceIcon } from "./workspace-navigation.js";

export async function openChannelSearch(state, open, select) {
  await open(
    "Find a channel",
    (parent) => {
      const input = createField(parent, "Search channels", "search", "", "Search all workspaces…");
      input.className = "channel-search";
      input.maxLength = 80;
      const summary = createElement("p", "hint");
      summary.setAttribute("role", "status");
      const results = createElement("div", "switcher-results");
      parent.append(summary, results);
      const normalize = (text) => text.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
      const render = () => {
        results.replaceChildren();
        for (const account of state.accounts) {
          const placement = state.organization.placements[account.id];
          const workspace = state.organization.workspaces.find(
            (w) => w.id === placement.workspaceId,
          );
          const category = workspace.categories.find((c) => c.id === placement.categoryId);
          const location = [workspace.name, category?.name].filter(Boolean).join(" / ");
          if (!normalize(`${account.name} ${location}`).includes(normalize(input.value.trim())))
            continue;
          const row = createButton("", () => select(account.id), "switcher-result");
          const label = createElement("span");
          label.append(
            createElement("strong", "", account.name),
            createElement("small", "", location),
          );
          row.append(serviceIcon(placement.service), label);
          results.append(row);
        }
        summary.textContent = results.childElementCount
          ? `${results.childElementCount} channels · Search stays on this device`
          : "No channels found. Try another name or workspace.";
      };
      input.addEventListener("input", render);
      input.addEventListener("keydown", (event) => {
        if (["ArrowDown", "Enter"].includes(event.key)) {
          event.preventDefault();
          if (event.key === "Enter") results.firstElementChild?.click();
          else results.firstElementChild?.focus();
        }
      });
      results.addEventListener("keydown", (event) => {
        if (!["ArrowDown", "ArrowUp"].includes(event.key)) return;
        event.preventDefault();
        const rows = Array.from(results.children);
        const index = rows.indexOf(document.activeElement);
        rows[(index + (event.key === "ArrowDown" ? 1 : rows.length - 1)) % rows.length]?.focus();
      });
      render();
    },
    null,
  );
}
