// Accept only drags started in this shell, never external payloads.
export function initializeChannelDrag(move, report, reorderWorkspaces) {
  let source = null;
  let target = null;
  let destination = null;
  let pending = false;
  const clear = () => {
    target?.classList.remove("drop-target", "drop-before", "drop-after");
    target = null;
    destination = null;
  };
  const itemAt = (element) =>
    element.closest?.('.account-row[draggable="true"], .workspace-icon[draggable="true"]');
  const finish = async (item, destination) => {
    if (pending) return;
    pending = true;
    try {
      if (item.dataset.workspaceId) {
        await reorderWorkspaces(item.dataset.workspaceId, destination.beforeId);
      } else {
        await move(
          item.dataset.accountId,
          destination.workspaceId,
          destination.categoryId,
          destination.beforeId,
        );
      }
    } catch (error) {
      report(error);
    } finally {
      pending = false;
    }
  };
  document.addEventListener("dragstart", (event) => {
    source = pending ? null : itemAt(event.target);
    if (!source) return;
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("text/plain", "Elo Talks item");
  });
  document.addEventListener("dragover", (event) => {
    if (!source || pending) return;
    clear();
    const workspaceDrag = !!source.dataset.workspaceId;
    const row = event.target.closest?.(
      workspaceDrag ? '.workspace-icon[draggable="true"]' : ".account-row",
    );
    const group = event.target.closest?.('[data-drop-workspace]:not([data-managed="true"])');
    if ((workspaceDrag && !row) || (!workspaceDrag && !group)) return;
    if (row === source) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = "move";
    target = row || group;
    if (row) {
      const after =
        event.clientY > row.getBoundingClientRect().top + row.getBoundingClientRect().height / 2;
      const siblings = [...row.parentElement.children].filter(
        (item) =>
          item.matches(workspaceDrag ? '.workspace-icon[draggable="true"]' : ".account-row") &&
          item !== source,
      );
      const next = after ? siblings[siblings.indexOf(row) + 1] : row;
      destination = {
        beforeId: next?.dataset[workspaceDrag ? "workspaceId" : "accountId"] || null,
      };
      target.classList.add(after ? "drop-after" : "drop-before");
    } else {
      destination = { beforeId: null };
      target.classList.add("drop-target");
    }
    if (!workspaceDrag) {
      destination.workspaceId = group.dataset.dropWorkspace;
      destination.categoryId = group.dataset.dropCategory || null;
    }
  });
  document.addEventListener("drop", (event) => {
    if (!source || !destination || pending) return;
    event.preventDefault();
    const item = source,
      next = destination;
    source = null;
    clear();
    void finish(item, next);
  });
  document.addEventListener("dragend", () => {
    source = null;
    clear();
  });
  document.addEventListener("keydown", (event) => {
    if (!event.altKey || !["ArrowUp", "ArrowDown"].includes(event.key)) return;
    const item = itemAt(event.target);
    if (!item || pending) return;
    event.preventDefault();
    const workspace = !!item.dataset.workspaceId;
    const siblings = [...item.parentElement.children].filter((row) =>
      row.matches(workspace ? '.workspace-icon[draggable="true"]' : ".account-row"),
    );
    const index = siblings.indexOf(item);
    const offset = event.key === "ArrowUp" ? -1 : 1;
    if (!siblings[index + offset]) return;
    const next = siblings[index + (offset < 0 ? -1 : 2)];
    const group = item.closest("[data-drop-workspace]");
    const selector = workspace
      ? `[data-workspace-id="${item.dataset.workspaceId}"]`
      : `.account-row[data-account-id="${item.dataset.accountId}"] .account-name`;
    void finish(item, {
      beforeId: next?.dataset[workspace ? "workspaceId" : "accountId"] || null,
      workspaceId: group.dataset.dropWorkspace,
      categoryId: group.dataset.dropCategory || null,
    }).then(() => document.querySelector(selector)?.focus());
  });
}
