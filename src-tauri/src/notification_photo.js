async (source) => {
  if (
    location.origin !== "https://web.whatsapp.com" ||
    typeof source !== "string" ||
    source.length > 4096
  )
    return "";
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 900);
  let bitmap;
  try {
    const url = new URL(source, location.href);
    const local = url.protocol === "blob:" && url.origin === location.origin;
    const embedded =
      url.protocol === "data:" && /^data:image\/(png|jpeg|webp);base64,/.test(source);
    const remote =
      url.protocol === "https:" &&
      !url.username &&
      !url.password &&
      (!url.port || url.port === "443") &&
      (url.hostname === "web.whatsapp.com" || url.hostname.endsWith(".whatsapp.net"));
    if (!local && !embedded && !remote) return "";
    const response = await fetch(url.href, {
      credentials: "omit",
      redirect: "error",
      cache: "force-cache",
      signal: controller.signal,
    });
    if (!response.ok || !response.body) return "";
    const reader = response.body.getReader();
    const chunks = [];
    let length = 0;
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      length += value.length;
      if (length > 65536) {
        await reader.cancel();
        return "";
      }
      chunks.push(value);
    }
    const bytes = new Uint8Array(length);
    let offset = 0;
    for (const chunk of chunks) {
      bytes.set(chunk, offset);
      offset += chunk.length;
    }
    const dimensions = () => {
      const view = new DataView(bytes.buffer);
      if (length >= 24 && view.getUint32(0) === 0x89504e47 && view.getUint32(4) === 0x0d0a1a0a)
        return [view.getUint32(16), view.getUint32(20)];
      if (length >= 4 && bytes[0] === 255 && bytes[1] === 216) {
        for (let pos = 2; pos + 9 < length; ) {
          if (bytes[pos++] !== 255) return null;
          while (bytes[pos] === 255) pos++;
          const marker = bytes[pos++];
          if (marker === 217 || marker === 218 || pos + 7 >= length) return null;
          const size = view.getUint16(pos);
          if (size < 2 || pos + size > length) return null;
          if ([192, 193, 194, 195, 197, 198, 199, 201, 202, 203, 205, 206, 207].includes(marker))
            return [view.getUint16(pos + 5), view.getUint16(pos + 3)];
          pos += size;
        }
      }
      if (length >= 30 && view.getUint32(0) === 0x52494646 && view.getUint32(8) === 0x57454250) {
        const kind = view.getUint32(12);
        if (kind === 0x56503858)
          return [
            1 + bytes[24] + (bytes[25] << 8) + (bytes[26] << 16),
            1 + bytes[27] + (bytes[28] << 8) + (bytes[29] << 16),
          ];
        if (kind === 0x56503820 && bytes[23] === 157 && bytes[24] === 1 && bytes[25] === 42)
          return [view.getUint16(26, true) & 16383, view.getUint16(28, true) & 16383];
        if (kind === 0x5650384c && bytes[20] === 47)
          return [
            1 + bytes[21] + ((bytes[22] & 63) << 8),
            1 + (bytes[22] >> 6) + (bytes[23] << 2) + ((bytes[24] & 15) << 10),
          ];
      }
      return null;
    };
    const size = dimensions();
    if (!size || size.some((value) => value <= 0 || value > 1024)) return "";
    bitmap = await createImageBitmap(new Blob([bytes]), {
      resizeWidth: 64,
      resizeHeight: 64,
      resizeQuality: "high",
    });
    const canvas = new OffscreenCanvas(64, 64);
    canvas.getContext("2d").drawImage(bitmap, 0, 0);
    const png = new Uint8Array(
      await (await canvas.convertToBlob({ type: "image/png" })).arrayBuffer(),
    );
    if (png.length > 32768) return "";
    return "data:image/png;base64," + btoa(String.fromCharCode(...png));
  } catch {
    return "";
  } finally {
    clearTimeout(timer);
    controller.abort();
    bitmap?.close();
  }
};
