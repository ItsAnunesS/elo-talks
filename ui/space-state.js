// This module is written only by the unlocked shell, whose storage is in the vault.
const key = "elo-space";
export function readSpace() {
  try {
    const value = JSON.parse(localStorage.getItem(key) || "{}");
    return value && typeof value === "object" ? value : {};
  } catch {
    return {};
  }
}
export function saveSpace(change) {
  if (document.body.dataset.unlocked !== "true") return;
  try {
    localStorage.setItem(key, JSON.stringify({ ...readSpace(), ...change }));
  } catch {
    window.dispatchEvent(
      new CustomEvent("ui-error", { detail: "Could not remember this workspace preference." }),
    );
  }
}
export function savedIds(field) {
  const value = readSpace()[field];
  return new Set(
    Array.isArray(value)
      ? value.filter((id) => typeof id === "string" && id.length <= 128).slice(0, 1000)
      : [],
  );
}
export function isFavorite(id) {
  return savedIds("favorites").has(id);
}
export function toggleFavorite(id) {
  const ids = savedIds("favorites");
  if (ids.has(id)) ids.delete(id);
  else ids.add(id);
  saveSpace({ favorites: [...ids] });
}
