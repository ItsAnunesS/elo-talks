// Read only the rendered self-profile control in an isolated WebKit script world.
// No fetching, storage, WhatsApp internals, contact avatars or message access.
(() => {
  if (window.top !== window || location.origin !== "https://web.whatsapp.com") return "";
  const selectors = [
    '#app [data-testid="menu-bar-profile"] img',
    '#app [data-testid="navbar"] [aria-label="Profile"] img',
    '#app [data-testid="navbar"] [aria-label="Perfil"] img',
    '#app [role="navigation"] [aria-label="Profile"] img',
    '#app [role="navigation"] [aria-label="Perfil"] img',
  ];
  // Ambiguous layouts fail closed rather than accidentally selecting a contact.
  const matches = new Set(
    selectors.flatMap((selector) => [...document.querySelectorAll(selector)]),
  );
  if (matches.size !== 1) return "";
  const image = [...matches][0];
  if (
    !image.complete ||
    !image.naturalWidth ||
    !image.naturalHeight ||
    image.naturalWidth > 4096 ||
    image.naturalHeight > 4096 ||
    image.closest('#main, [data-testid="conversation-panel-wrapper"], [role="dialog"]') ||
    image.getClientRects().length === 0
  )
    return "";
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
    return "";
  }
})();
