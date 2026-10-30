const overlay = document.getElementById("startup-screen");
const label = document.getElementById("startup-label");
export function showLoading(text) {
  label.textContent = text;
  overlay.hidden = false;
  for (const id of ["workspace", "lock-screen"]) document.getElementById(id).inert = true;
}
export function hideLoading() {
  overlay.hidden = true;
  for (const id of ["workspace", "lock-screen"]) document.getElementById(id).inert = false;
}
