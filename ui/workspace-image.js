// Decode a user-selected raster file locally and export only fixed-size pixels.
// The controller encodes a fresh PNG, stripping source metadata and animation.
export async function workspaceImagePixels(file) {
  if (!file || !["image/png", "image/jpeg", "image/webp"].includes(file.type))
    throw Error("Choose a PNG, JPEG or WebP image.");
  if (file.size > 5 * 1024 * 1024) throw Error("Choose an image smaller than 5 MB.");
  const header = new Uint8Array(await file.slice(0, 12).arrayBuffer());
  const matches = (bytes, offset = 0) =>
    bytes.every((byte, index) => header[offset + index] === byte);
  const valid = {
    "image/png": matches([137, 80, 78, 71, 13, 10, 26, 10]),
    "image/jpeg": matches([255, 216, 255]),
    "image/webp": matches([82, 73, 70, 70]) && matches([87, 69, 66, 80], 8),
  };
  if (!valid[file.type]) throw Error("File contents do not match the image type.");
  const source = await new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result);
    reader.onerror = () => reject(Error("Could not read the image."));
    reader.readAsDataURL(file);
  });
  const image = new Image();
  try {
    image.src = source;
    await image.decode();
    if (
      !image.naturalWidth ||
      !image.naturalHeight ||
      image.naturalWidth > 4096 ||
      image.naturalHeight > 4096
    )
      throw Error("Choose an image up to 4096 × 4096 pixels.");
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
    return {
      pixels: Array.from(context.getImageData(0, 0, 64, 64).data, (byte) =>
        byte.toString(16).padStart(2, "0"),
      ).join(""),
      preview: canvas.toDataURL("image/png"),
    };
  } finally {
    image.removeAttribute("src");
  }
}
