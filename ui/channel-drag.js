// Only drags started on a local channel row are accepted. Ignore external payloads.
export function initializeChannelDrag(move, report) {
  let source = null;
  let target = null;
  let pending = false;
  const clear = () => {
    target?.classList.remove("drop-target");
    target = null;
  };
  document.addEventListener("dragstart", (event) => {
    const row = event.target.closest?.(".account-row[data-account-id]");
    if (!row || pending) {
      source = null;
      return;
    }
    source = row.dataset.accountId;
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData("text/plain", "Elo Talks channel");
  });
  document.addEventListener("dragover", (event) => {
    if (!source || pending) return;
    const next = event.target.closest?.("[data-drop-workspace]");
    if (!next) {
      clear();
      return;
    }
    event.preventDefault();
    event.dataTransfer.dropEffect = "move";
    if (target !== next) {
      clear();
      target = next;
      target.classList.add("drop-target");
    }
  });
  document.addEventListener("drop", async (event) => {
    const next = event.target.closest?.("[data-drop-workspace]");
    if (!source || !next || pending) return;
    event.preventDefault();
    const id = source;
    source = null;
    clear();
    pending = true;
    try {
      await move(id, next.dataset.dropWorkspace, next.dataset.dropCategory || null);
    } catch (error) {
      report(error);
    } finally {
      pending = false;
    }
  });
  document.addEventListener("dragend", () => {
    source = null;
    clear();
  });
}
