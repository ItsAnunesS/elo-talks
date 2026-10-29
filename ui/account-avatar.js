// Disposable UI memory only. Never store avatar data in browser or vault storage.
const avatars = new Map();
let accountIds = new Set();

function render(id) {
  for (const element of document.querySelectorAll("[data-avatar-account]")) {
    if (element.dataset.avatarAccount !== id) continue;
    const source = avatars.get(id);
    const current = element.querySelector("img");
    if (!source) {
      current?.remove();
      element.classList.remove("has-avatar");
      continue;
    }
    if (current?.src === source) continue;
    const image = new Image(64, 64);
    image.alt = "";
    image.draggable = false;
    image.addEventListener(
      "error",
      () => {
        image.remove();
        element.classList.remove("has-avatar");
      },
      { once: true },
    );
    image.src = source;
    current?.remove();
    element.append(image);
    element.classList.add("has-avatar");
  }
}

export function syncAccountAvatars(accounts) {
  accountIds = new Set(accounts.map((account) => account.id));
  for (const id of avatars.keys()) if (!accountIds.has(id)) avatars.delete(id);
  for (const id of accountIds) render(id);
}

export function receiveAccountAvatar({ id, pixels }) {
  if (!accountIds.has(id)) return;
  if (
    !Array.isArray(pixels) ||
    pixels.length !== 64 * 64 * 4 ||
    !pixels.every((byte) => Number.isInteger(byte) && byte >= 0 && byte <= 255)
  ) {
    avatars.delete(id);
    render(id);
    return;
  }
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = 64;
  const context = canvas.getContext("2d");
  if (!context) return;
  context.putImageData(new ImageData(new Uint8ClampedArray(pixels), 64, 64), 0, 0);
  avatars.set(id, canvas.toDataURL("image/png"));
  render(id);
}
