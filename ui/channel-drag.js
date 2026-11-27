// Pointer gestures avoid native HTML5 drag-and-drop failures in Linux CEF.
export function initializeChannelDrag(move, report, reorderWorkspaces) {
  let source = null;
  let target = null;
  let destination = null;
  let pending = false;
  let gesture = null;
  let suppressClick = false;
  let scrollFrame = null;
  const clear = () => {
    target?.classList.remove("drop-target", "drop-before", "drop-after");
    target = null;
    destination = null;
  };
  const itemAt = (element) =>
    element.closest?.(
      '.account-row[data-reorderable="true"], .workspace-icon[data-reorderable="true"]',
    );
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
  const updateTarget = (element, y) => {
    clear();
    const workspaceDrag = !!source.dataset.workspaceId;
    const row = element?.closest?.(
      workspaceDrag ? '.workspace-icon[data-reorderable="true"]' : ".account-row",
    );
    const group = element?.closest?.('[data-drop-workspace]:not([data-managed="true"])');
    if ((workspaceDrag && !row) || (!workspaceDrag && !group) || row === source) return;
    target = row || group;
    if (row) {
      const rect = row.getBoundingClientRect();
      const after = y > rect.top + rect.height / 2;
      const siblings = [...row.parentElement.children].filter(
        (item) =>
          item.matches(
            workspaceDrag ? '.workspace-icon[data-reorderable="true"]' : ".account-row",
          ) && item !== source,
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
  };
  const cancel = () => {
    const item = source,
      pointerId = gesture?.pointerId;
    source = null;
    gesture = null;
    cancelAnimationFrame(scrollFrame);
    scrollFrame = null;
    item?.classList.remove("is-dragging");
    document.body.classList.remove("navigation-dragging");
    if (item?.hasPointerCapture(pointerId)) item.releasePointerCapture(pointerId);
    clear();
  };
  const autoScroll = () => {
    if (!gesture?.active || !source?.isConnected) {
      cancel();
      return;
    }
    const element = document.elementFromPoint(gesture.x, gesture.y);
    const scroller = element?.closest("#account-list, #workspace-icons");
    if (scroller) {
      const rect = scroller.getBoundingClientRect();
      const direction = gesture.y < rect.top + 28 ? -1 : gesture.y > rect.bottom - 28 ? 1 : 0;
      if (direction) {
        scroller.scrollTop += direction * 6;
        updateTarget(document.elementFromPoint(gesture.x, gesture.y), gesture.y);
      }
    }
    scrollFrame = requestAnimationFrame(autoScroll);
  };
  document.addEventListener("pointerdown", (event) => {
    if (event.button !== 0 || !event.isPrimary) return;
    suppressClick = false;
    if (gesture) cancel();
    if (pending || event.target.closest?.(".account-menu")) return;
    source = itemAt(event.target);
    if (!source) return;
    gesture = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      x: event.clientX,
      y: event.clientY,
      active: false,
    };
  });
  document.addEventListener("pointermove", (event) => {
    if (!gesture || event.pointerId !== gesture.pointerId) return;
    if (!source.isConnected || !(event.buttons & 1)) {
      cancel();
      return;
    }
    gesture.x = event.clientX;
    gesture.y = event.clientY;
    if (!gesture.active) {
      if (Math.hypot(gesture.x - gesture.startX, gesture.y - gesture.startY) < 5) return;
      gesture.active = true;
      source.setPointerCapture(event.pointerId);
      source.classList.add("is-dragging");
      document.body.classList.add("navigation-dragging");
      scrollFrame = requestAnimationFrame(autoScroll);
    }
    event.preventDefault();
    updateTarget(document.elementFromPoint(event.clientX, event.clientY), event.clientY);
  });
  document.addEventListener("pointerup", (event) => {
    if (!gesture || event.pointerId !== gesture.pointerId) return;
    if (!gesture.active || !source.isConnected) {
      cancel();
      return;
    }
    event.preventDefault();
    updateTarget(document.elementFromPoint(event.clientX, event.clientY), event.clientY);
    const item = source,
      next = destination;
    suppressClick = true;
    cancel();
    if (next) void finish(item, next);
  });
  document.addEventListener("pointercancel", (event) => {
    if (event.pointerId === gesture?.pointerId) cancel();
  });
  document.addEventListener("lostpointercapture", (event) => {
    if (event.pointerId === gesture?.pointerId) cancel();
  });
  window.addEventListener("blur", cancel);
  document.addEventListener("dragstart", (event) => {
    if (itemAt(event.target)) event.preventDefault();
  });
  document.addEventListener(
    "click",
    (event) => {
      if (!suppressClick || event.detail === 0) return;
      suppressClick = false;
      event.preventDefault();
      event.stopImmediatePropagation();
    },
    true,
  );
  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && gesture) {
      event.preventDefault();
      suppressClick = gesture.active;
      cancel();
      return;
    }
    if (!event.altKey || !["ArrowUp", "ArrowDown"].includes(event.key)) return;
    const item = itemAt(event.target);
    if (!item || pending || gesture) return;
    event.preventDefault();
    const workspace = !!item.dataset.workspaceId;
    const siblings = [...item.parentElement.children].filter((row) =>
      row.matches(workspace ? '.workspace-icon[data-reorderable="true"]' : ".account-row"),
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
