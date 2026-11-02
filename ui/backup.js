import { createElement, createField, getElement, errorMessage } from "./dom.js";
import { invoke, request } from "./bridge.js";

let pollTimer;
let watching = false;

export function watchBackup(status, refresh) {
  const output = getElement("backup-status");
  if (!status || status.phase === "idle") {
    output.hidden = true;
    return;
  }
  output.hidden = false;
  output.textContent =
    status.phase === "running"
      ? "Processing encrypted backup. Keep Elo Talks open."
      : status.phase === "error"
        ? status.error
        : status.operation === "import"
          ? "Backup imported. Unlock with your backup password. Set up your PIN and security keys again."
          : "Encrypted backup saved. You can unlock your vault again.";
  if (status.phase !== "running" || watching) return;
  watching = true;
  const poll = async () => {
    try {
      const current = await request("backup.status");
      if (current.phase === "running") {
        pollTimer = setTimeout(poll, 750);
        return;
      }
      watching = false;
      await refresh();
    } catch (error) {
      watching = false;
      output.textContent = `${errorMessage(error)} Reopen Elo Talks to check the vault before retrying.`;
    }
  };
  clearTimeout(pollTimer);
  pollTimer = setTimeout(poll, 750);
}

export async function backupDialog({ created, openDialog, feedback }) {
  let password;
  let backupPassword;
  let confirmation;
  await openDialog(
    created ? "Export encrypted backup" : "Import encrypted backup",
    (parent) => {
      parent.append(
        createElement(
          "p",
          "",
          created
            ? "Save your workspaces, categories, channels and complete account profiles in a password-protected file. Your vault stays locked."
            : "Restore a backup into this new installation. The backup password becomes your master password. PIN and security keys must be set up again.",
        ),
        createElement(
          "p",
          "hint",
          "This is a snapshot, not synchronization. WhatsApp may require linking again. Avoid running the original and restored copies of the same session together.",
        ),
      );
      password = createField(
        parent,
        created ? "Current master password" : "Backup password",
        "password",
      );
      password.required = true;
      password.maxLength = 1024;
      if (created) {
        backupPassword = createField(
          parent,
          "Backup password",
          "password",
          "",
          "At least 12 characters",
        );
        backupPassword.required = true;
        backupPassword.minLength = 12;
        backupPassword.maxLength = 1024;
        confirmation = createField(parent, "Confirm backup password", "password");
        confirmation.required = true;
        confirmation.maxLength = 1024;
      }
    },
    async () => {
      if (created && backupPassword.value !== confirmation.value)
        throw Error("The backup passwords do not match.");
      const path = await invoke("choose_backup", { export: created });
      if (!path) throw Error("No file selected. Choose a file to continue or cancel.");
      try {
        await request(created ? "backup.export" : "backup.import", {
          path,
          password: password.value,
          ...(created ? { backupPassword: backupPassword.value } : {}),
        });
      } finally {
        password.value = "";
        if (backupPassword) backupPassword.value = "";
        if (confirmation) confirmation.value = "";
      }
      feedback("Encrypted backup started.");
      // The shared dialog refreshes state and closes after this action resolves.
    },
    created ? "Choose destination" : "Choose backup",
  );
}
