// Read only the rendered self-profile control in an isolated WebKit script world.
// No fetching, storage, WhatsApp internals, contact avatars or message access.
(() => {
  if (window.top !== window || location.origin !== "https://web.whatsapp.com")
    return "wrong-origin";
  const profileControls = [
    '#app [data-testid="menu-bar-profile"]',
    '#app :is(header, nav, [role="navigation"], [data-testid="navbar"]) :is(button, [role="button"]):is([aria-label="Profile"], [aria-label="Perfil"])',
    '#app :is([role="navigation"], [data-testid="navbar"]) :is([aria-label="Profile"], [aria-label="Perfil"])',
  ];
  const excluded =
    '#main, #side, #pane-side, [data-testid="conversation-panel-wrapper"], [role="dialog"], [role="listitem"], [role="row"]';
  const visible = (element) => {
    if (!element.getClientRects().length) return false;
    for (let node = element; node; node = node.parentElement) {
      const style = getComputedStyle(node);
      if (style.display === "none" || style.visibility !== "visible" || Number(style.opacity) === 0)
        return false;
    }
    return true;
  };
  const matches = new Set();
  let foundControl = false;
  for (const selector of profileControls) {
    for (const control of document.querySelectorAll(selector)) {
      if (control.closest(excluded) || !visible(control)) continue;
      foundControl = true;
      for (const image of control.querySelectorAll("img")) {
        if (!image.closest(excluded) && visible(image)) matches.add(image);
      }
    }
  }
  // Hidden responsive copies are ignored; multiple visible candidates remain ambiguous.
  if (matches.size > 1) return "ambiguous";
  if (!matches.size) return foundControl ? "no-photo" : "control-not-found";
  const image = [...matches][0];
  if (!image.complete || !image.naturalWidth || !image.naturalHeight) return "image-not-ready";
  if (image.naturalWidth > 4096 || image.naturalHeight > 4096) return "image-too-large";
  try {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 64;
    const context = canvas.getContext("2d", { willReadFrequently: true });
    const size = Math.min(image.naturalWidth, image.naturalHeight);
    context.drawImage(
      image,
      (image.naturalWidth - size) / 2,
      (image.naturalHeight - size) / 2,
      size,
      size,
      0,
      0,
      64,
      64,
    );
    // Raw pixels only: no URL, encoded image format, HTML or executable data crosses the boundary.
    return Array.from(context.getImageData(0, 0, 64, 64).data, (byte) =>
      byte.toString(16).padStart(2, "0"),
    ).join("");
  } catch {
    // Respect canvas origin protections; never fetch again or bypass CORS.
    return "canvas-blocked";
  }
})();
