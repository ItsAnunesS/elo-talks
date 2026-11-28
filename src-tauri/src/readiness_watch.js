(() => {
  if (location.origin !== "https://web.whatsapp.com" || window !== window.top) return false;
  if (globalThis.eloReadinessWatch) return true;
  globalThis.eloReadinessWatch = true;
  const detect = () => __READINESS_QUERY__;
  let pending = null;
  let previous = -1;
  let chats = null;
  const update = () => {
    pending = null;
    const state = detect();
    chats = state === 1 ? document.getElementById("pane-side") : null;
    if (state !== previous && typeof globalThis.__eloReadiness === "function") {
      globalThis.__eloReadiness(String(state));
      previous = state;
    }
  };
  const schedule = () => {
    if (pending === null) pending = setTimeout(update, 200);
  };
  new MutationObserver((records) => {
    // Ordinary updates inside a connected chat list cannot change login state.
    if (
      previous === 1 &&
      chats?.isConnected &&
      records.length <= 32 &&
      records.every((record) => record.target !== chats && chats.contains(record.target))
    )
      return;
    schedule();
  }).observe(document, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["class", "style", "hidden", "aria-hidden"],
  });
  addEventListener("resize", schedule);
  addEventListener("pageshow", schedule);
  document.addEventListener("visibilitychange", schedule);
  update();
  return true;
})();
