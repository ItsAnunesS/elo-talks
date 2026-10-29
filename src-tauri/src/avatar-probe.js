(async () => {
  // Read only the rendered self-profile control in an isolated WebKit script world.
  // No storage, WhatsApp internals, contact avatars or message access.
  // A bounded, credential-free CORS read may recover an origin-tainted self-photo.
  if (window.top !== window || location.origin !== "https://web.whatsapp.com")
    return "wrong-origin";
  const navigation =
    '#app :is(header, nav, [role="navigation"], [role="tablist"], [data-testid="navbar"])';
  // WhatsApp also labels its self-profile entry "You" / "Você".
  // Match whole labels only, never user names, generic avatars or positional images.
  const profileLabels = ["You", "Você", "Profile", "Perfil", "Your profile", "Seu perfil"];
  const labeledProfile = profileLabels
    .flatMap((label) => [`[aria-label="${label}"]`, `[title="${label}"]`])
    .join(", ");
  const profileControls = [
    '#app [data-testid="menu-bar-profile"]',
    `${navigation} :is(${labeledProfile})`,
    '#app [data-navbar-item="profile"]',
    `#app [data-navbar-item]:is(${labeledProfile})`,
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
  const cacheKey = "__eloSelfAvatarCors";
  const discardCache = () => {
    globalThis[cacheKey]?.controller.abort();
    delete globalThis[cacheKey];
  };
  const findImage = () => {
    const matches = new Set();
    let foundControl = false;
    const markers = new Set(document.querySelectorAll(profileControls.join(", ")));
    // Accessible names may reference a separate navigation label or tooltip.
    // Inspect only short leaf labels, never arbitrary subtree text or conversation content.
    for (const control of document.querySelectorAll(
      `${navigation} :is(${interactive})[aria-labelledby], ${navigation} :is(${interactive})[aria-describedby]`,
    )) {
      if (control.closest(excluded) || !visible(control)) continue;
      for (const attribute of ["aria-labelledby", "aria-describedby"]) {
        const references = control.getAttribute(attribute) || "";
        if (references.length > 256) continue;
        for (const id of references.trim().split(/\s+/).slice(0, 4)) {
          const label = document.getElementById(id);
          if (!label || label.childElementCount || label.closest(excluded)) continue;
          if (!label.closest(navigation) && label.getAttribute("role") !== "tooltip") continue;
          const text = label.textContent || "";
          if (text.length <= 64 && profileLabels.includes(text.trim())) markers.add(control);
        }
      }
    }
    for (const marker of markers) {
      if (marker.closest(excluded)) continue;
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
    // Hidden responsive copies are ignored; multiple visible candidates remain ambiguous.
    if (matches.size > 1) return "ambiguous";
    if (!matches.size) return foundControl ? "no-photo" : "control-not-found";
    return [...matches][0];
  };
  const image = findImage();
  if (typeof image === "string") {
    discardCache();
    return image;
  }
  if (!image.complete || !image.naturalWidth || !image.naturalHeight) return "image-not-ready";
  if (image.naturalWidth > 4096 || image.naturalHeight > 4096) return "image-too-large";
  const pixelsFrom = (image) => {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 64;
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (!context) throw new Error("canvas-unavailable");
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
  };
  try {
    const pixels = pixelsFrom(image);
    discardCache();
    return pixels;
  } catch (error) {
    if (error.name !== "SecurityError") return "canvas-unavailable";
  }
  const source = image.currentSrc || image.src;
  const url = new URL(source);
  const localBlob = url.protocol === "blob:" && url.origin === location.origin;
  const mediaImage =
    url.protocol === "https:" &&
    !url.username &&
    !url.password &&
    !url.port &&
    ["pps.whatsapp.net", "pps.whatsapp.com"].includes(url.hostname);
  if (!localBlob && !mediaImage) {
    discardCache();
    return "image-source-unsupported";
  }
  const previous = globalThis[cacheKey];
  if (!previous || previous.source !== source || Date.now() - previous.started > 300000) {
    discardCache();
    const controller = new AbortController();
    const entry = { source, controller, started: Date.now() };
    globalThis[cacheKey] = entry;
    entry.result = (async () => {
      const timeout = setTimeout(() => controller.abort(), 5000);
      let objectUrl, copy;
      try {
        // Respect CORS and TLS. Never forward cookies, follow redirects or persist bytes.
        const response = await fetch(source, {
          mode: "cors",
          credentials: "omit",
          cache: "no-store",
          redirect: "error",
          referrerPolicy: "no-referrer",
          signal: controller.signal,
        });
        const type = (response.headers.get("Content-Type") || "")
          .split(";")[0]
          .trim()
          .toLowerCase();
        if (!response.ok || !["image/jpeg", "image/png", "image/webp"].includes(type))
          return "image-request-failed";
        const limit = 512 * 1024;
        if (Number(response.headers.get("Content-Length")) > limit) {
          controller.abort();
          return "image-too-large";
        }
        if (!response.body) return "image-request-failed";
        const reader = response.body.getReader();
        const chunks = [];
        let size = 0;
        while (true) {
          const { done, value } = await reader.read();
          if (done) break;
          size += value.length;
          if (size > limit) {
            controller.abort();
            return "image-too-large";
          }
          chunks.push(value);
        }
        reader.releaseLock();
        const blob = new Blob(chunks, { type });
        const header = new Uint8Array(await blob.slice(0, 12).arrayBuffer());
        const matches = (bytes, offset = 0) =>
          bytes.every((byte, index) => header[offset + index] === byte);
        const raster = {
          "image/png": matches([137, 80, 78, 71, 13, 10, 26, 10]),
          "image/jpeg": matches([255, 216, 255]),
          "image/webp": matches([82, 73, 70, 70]) && matches([87, 69, 66, 80], 8),
        };
        if (!raster[type]) return "image-request-failed";
        objectUrl = URL.createObjectURL(blob);
        copy = new Image();
        copy.src = objectUrl;
        await copy.decode();
        if (copy.naturalWidth > 4096 || copy.naturalHeight > 4096) return "image-too-large";
        return pixelsFrom(copy);
      } catch (error) {
        return error.name === "SecurityError" ? "canvas-blocked" : "image-request-failed";
      } finally {
        controller.abort();
        clearTimeout(timeout);
        copy?.removeAttribute("src");
        if (objectUrl) URL.revokeObjectURL(objectUrl);
      }
    })();
  }
  const entry = globalThis[cacheKey];
  const pixels = await entry.result;
  // Logout, navigation changes and replacement photos invalidate an outstanding result.
  if (globalThis[cacheKey] !== entry) return "image-not-ready";
  if (!image.isConnected || (image.currentSrc || image.src) !== source || findImage() !== image) {
    discardCache();
    return "image-not-ready";
  }
  return pixels;
})();
