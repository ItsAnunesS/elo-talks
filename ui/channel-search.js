import { createButton, createElement, createField } from "./dom.js";
import { icon } from "./icons.js";
import { serviceIcon } from "./workspace-navigation.js";

export async function openChannelSearch(state, open, select, actions = []) {
  await open(
    "Find a channel",
    (parent) => {
      const input = createField(
        parent,
        "Search channels",
        "search",
        "",
        "Channels, workspaces or actions…",
      );
      input.className = "channel-search";
      input.maxLength = 80;
      const summary = createElement("p", "hint");
      summary.setAttribute("role", "status");
      const results = createElement("div", "switcher-results");
      parent.append(summary, results);
      const normalize = (text) => text.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
      const workspaces = new Map(
        state.organization.workspaces.map((workspace) => [workspace.id, workspace]),
      );
      const channels = state.accounts.flatMap((account) => {
        const placement = state.organization.placements[account.id];
        const workspace = workspaces.get(placement?.workspaceId);
        if (!workspace) return [];
        const category = workspace.categories.find((item) => item.id === placement.categoryId);
        const location = [workspace.name, category?.name].filter(Boolean).join(" / ");
        return [{ account, placement, location, search: normalize(`${account.name} ${location}`) }];
      });
      const shortcuts = actions.map((action) => ({
        ...action,
        search: normalize(`${action.label} ${action.hint || ""}`),
      }));
      const render = () => {
        const query = normalize(input.value.trim());
        const fragment = document.createDocumentFragment();
        for (const { account, placement, location, search } of channels) {
          if (!search.includes(query)) continue;
          const row = createButton("", () => select(account.id), "switcher-result");
          const label = createElement("span");
          label.append(
            createElement("strong", "", account.name),
            createElement("small", "", location),
          );
          row.append(serviceIcon(placement.service), label);
          fragment.append(row);
        }
        const channelCount = fragment.childElementCount;
        for (const action of shortcuts) {
          if (!action.search.includes(query)) continue;
          const row = createButton("", action.run, "switcher-result quick-action");
          const label = createElement("span");
          label.append(
            createElement("strong", "", action.label),
            createElement("small", "", action.hint || "Action"),
          );
          row.append(icon(action.icon || "arrow"), label);
          fragment.append(row);
        }
        results.replaceChildren(fragment);
        summary.textContent = results.childElementCount
          ? `${channelCount} channels · ${results.childElementCount - channelCount} shortcuts`
          : "No matches. Try another name.";
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
        if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
        event.preventDefault();
        const rows = Array.from(results.children);
        const index = rows.indexOf(document.activeElement);
        if (event.key === "ArrowUp" && index === 0) input.focus();
        else if (event.key === "Home") rows[0]?.focus();
        else if (event.key === "End") rows.at(-1)?.focus();
        else
          rows[(index + (event.key === "ArrowDown" ? 1 : rows.length - 1)) % rows.length]?.focus();
      });
      render();
    },
    null,
  );
}
