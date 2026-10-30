(() => {
  // Inspect structural presence only: no text, attributes, QR pixels or messages.
  const visible = (selector) => {
    const element = document.querySelector(selector);
    if (!element || !element.getClientRects().length || getComputedStyle(element).visibility !== "visible") return false;
    const bounds = element.getBoundingClientRect();
    if (bounds.width <= 0 || bounds.height <= 0) return false;
    const x = Math.max(0, Math.min(innerWidth - 1, bounds.left + bounds.width / 2));
    const y = Math.max(0, Math.min(innerHeight - 1, bounds.top + bounds.height / 2));
    // A splash screen covering an already-created chat list is still loading.
    const top = document.elementFromPoint(x, y);
    return !!top && element.contains(top);
  };
  if (visible('#pane-side')) return 1;
  if (visible('[data-ref] canvas') || visible('[data-testid="qrcode"] canvas')) return 2;
  return 0;
})()
