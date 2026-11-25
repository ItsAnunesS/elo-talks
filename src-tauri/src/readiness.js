(() => {
  // Inspect structural presence only: no text, attributes, QR pixels or messages.
  const visible = (element, owner = element) => {
    if (!element || !element.getClientRects().length || getComputedStyle(element).visibility !== "visible") return false;
    const bounds = element.getBoundingClientRect();
    if (bounds.width <= 0 || bounds.height <= 0) return false;
    const x = Math.max(0, Math.min(innerWidth - 1, bounds.left + bounds.width / 2));
    const y = Math.max(0, Math.min(innerHeight - 1, bounds.top + bounds.height / 2));
    // A splash screen covering an already-created chat list is still loading.
    const top = document.elementFromPoint(x, y);
    return !!top && owner.contains(top);
  };
  if (visible(document.querySelector('#pane-side'))) return 1;
  for (const selector of ['[data-ref]', '[data-testid="qrcode"]']) {
    const marker = document.querySelector(selector);
    if (!marker) continue;
    const code = marker.matches('canvas,img,svg') ? marker : marker.querySelector('canvas,img,svg');
    // A logo or refresh control inside the QR container does not hide login.
    if (visible(code, marker)) return 2;
  }
  return 0;
})()
