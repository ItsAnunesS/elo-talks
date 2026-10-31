const overlay = document.getElementById("startup-screen");
const label = document.getElementById("startup-label");
export function showLoading(text) {
  label.textContent = text;
  document.getElementById("update-progress").hidden = true;
  overlay.hidden = false;
  for (const id of ["workspace", "lock-screen"]) document.getElementById(id).inert = true;
}
export function hideLoading() {
  overlay.hidden = true;
  document.getElementById("update-progress").hidden = true;
  for (const id of ["workspace", "lock-screen"]) document.getElementById(id).inert = false;
}

export function showUpdate(status) {
  const step = status.progress?.step;
  const labels = {
    checking: "Checking for updates…",
    downloading: "Downloading update…",
    ready: "Update verified. Ready to install.",
    installing: "Installing and restarting…",
  };
  showLoading(
    step === "verifying-release"
      ? "Verifying release signature…"
      : step === "verifying-download"
        ? "Verifying download…"
        : labels[status.phase] || "Preparing update…",
  );
  const panel = document.getElementById("update-progress");
  panel.hidden = false;
  document.getElementById("update-version").textContent = [status.currentVersion, status.version]
    .filter(Boolean)
    .join(" → ");
  const meter = document.getElementById("update-meter");
  const downloaded = status.progress?.downloaded || 0;
  const total = status.progress?.total || 0;
  const detail = document.getElementById("update-detail");
  if (step === "downloading" && total > 0) {
    meter.value = Math.min(100, (downloaded / total) * 100);
    detail.textContent = `${(downloaded / 1048576).toFixed(1)} / ${(total / 1048576).toFixed(1)} MB`;
  } else {
    meter.removeAttribute("value");
    detail.textContent =
      status.phase === "installing"
        ? "Your vault is closed before replacement."
        : "The signature and checksum must pass before installation.";
  }
}
