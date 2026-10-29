// Read only the rendered self-profile control in an isolated WebKit script world.
// No fetching, storage, WhatsApp internals, contact avatars or message access.
(() => {
  if (window.top !== window || location.origin !== "https://web.whatsapp.com")
    return "wrong-origin";
  const navigation =
    '#app :is(header, nav, [role="navigation"], [role="tablist"], [data-testid="navbar"])';
  // WhatsApp also labels its self-profile entry "You" / "Você".
  // Match whole labels only, never user names, generic avatars or positional images.
  const profileLabels = ["You", "Você", "Profile", "Perfil"];
  const labeledProfile = profileLabels
    .flatMap((label) => [`[aria-label="${label}"]`, `[title="${label}"]`])
    .join(", ");
  const profileControls = [
    '#app [data-testid="menu-bar-profile"]',
    `${navigation} :is(${labeledProfile})`,
    `${navigation} img:is([alt="Your profile picture"], [alt="Sua foto de perfil"])`,
  ];
  const interactive = 'button, [role="button"], [role="tab"]';
  const excluded =
    '#main, #side, #pane-side, [data-testid="conversation-panel-wrapper"], [role="dialog"], [role="listitem"], [role="row"], [role="listbox"], [role="menu"], [contenteditable="true"]';
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
    for (const marker of document.querySelectorAll(selector)) {
      if (marker.closest(excluded) || !visible(marker)) continue;
      // A tooltip label can be on a child of the button rather than the button itself.
      const parentControl = marker.closest(interactive);
      const control = parentControl?.closest(navigation) ? parentControl : marker;
      if (control.closest(excluded) || !visible(control)) continue;
      foundControl = true;
      const images = control.matches("img") ? [control] : control.querySelectorAll("img");
      for (const image of images) {
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
