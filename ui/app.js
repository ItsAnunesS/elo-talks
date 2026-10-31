import { updateSettings } from "./update-settings.js";
import { initializeUpdates, restartForUpdate } from "./updates.js";
import { prepareAccounts } from "./startup.js";
import { showLoading, hideLoading } from "./loading-screen.js";
import {
  getElement,
  errorMessage,
  createElement,
  createButton,
  createField,
  createTabs,
  createDestructiveAction,
} from "./dom.js";
import { invoke, request, listen, isDesktop } from "./bridge.js";
import { updateAccountStatus } from "./account-status.js";
import {
  renderNavigation,
  clearWorkspaceImages,
  revealChannel,
  updateAvatar,
  refreshAvatars,
  renderChannelIdentity,
} from "./workspace-navigation.js";
import { placementFields, workspaceDialogs } from "./workspace-dialogs.js";
import { hydrateIcons } from "./icons.js";
import { openChannelSearch } from "./channel-search.js";
import { initializePinInputs } from "./pin-input.js";
hydrateIcons();
const digits = initializePinInputs(getElement("pin-inputs"));
let state = { created: false, unlocked: false, pin: false, keys: [], accounts: [], idleMinutes: 0 };
let startingAccounts = false;
let pendingOpenAccount = null;
let method = "password",
  selected = null,
  activeWorkspace = null,
  restoreSelected = null,
  dialogAction,
  dialogFocus,
  dialogSubmitting = false,
  dialogOpening = false,
  toastTimer;
const { editWorkspace, editCategory } = workspaceDialogs({
  state: () => state,
  active: () => activeWorkspace,
  open: openDialog,
  close: closeDialog,
  refresh: refreshState,
  created: (id) => {
    activeWorkspace = id;
    selected = null;
    restoreSelected = null;
  },
});
function showToast(text) {
  clearTimeout(toastTimer);
  const inDialog = getElement("dialog").open;
  const output = getElement(inDialog ? "dialog-feedback" : "toast");
  getElement(inDialog ? "toast" : "dialog-feedback").hidden = true;
  output.textContent = text;
  output.hidden = false;
  if (!inDialog)
    toastTimer = setTimeout(() => {
      output.hidden = true;
    }, 5000);
}
window.addEventListener("ui-error", (event) => showToast(event.detail));
window.addEventListener("update-failed", () => {
  if (state.unlocked && !getElement("dialog").open) {
    void selectAccount(selected).catch((error) => showToast(errorMessage(error)));
  }
});
function clearSecrets() {
  getElement("secret").value = "";
  getElement("confirm-secret").value = "";
  document.querySelectorAll(".pin-inputs input").forEach((n) => (n.value = ""));
}
function selectUnlockMethod(next) {
  method = next;
  clearSecrets();
  document
    .querySelectorAll("[data-method]")
    .forEach((b) => b.classList.toggle("active", b.dataset.method === method));
  getElement("secret").hidden = method === "pin";
  getElement("secret").required = method === "password";
  getElement("secret-label").textContent =
    method === "pin"
      ? "PIN for this computer"
      : method === "key"
        ? "Security key PIN (if required)"
        : "Your password";
  getElement("secret").placeholder = method === "key" ? "Authenticator PIN" : "Enter your password";
  getElement("pin-inputs").hidden = method !== "pin";
  getElement("key-choice").hidden = method !== "key";
  getElement("unlock-hint").textContent =
    method === "pin"
      ? "This PIN only works with this computer’s keyring."
      : method === "key"
        ? "Connect one compatible security key and touch it when prompted."
        : "Your password unlocks the encrypted vault on this computer.";
}
document
  .querySelectorAll("[data-method]")
  .forEach((b) => b.addEventListener("click", () => selectUnlockMethod(b.dataset.method)));
async function refreshState() {
  state = await request("status");
  renderState();
  if (state.unlocked) {
    await invoke("sync_accounts");
    updateAccountStatus(await invoke("account_snapshot"), selected);
  }
}
function renderState() {
  document.body.dataset.unlocked = String(state.unlocked);
  getElement("lock-screen").hidden = state.unlocked;
  getElement("workspace").hidden = !state.unlocked;
  getElement("version").textContent = `v${state.version || "development"}`;
  const quiet = !!state.notifications?.doNotDisturb;
  getElement("quiet-button").setAttribute("aria-pressed", String(quiet));
  getElement("quiet-button").title = quiet
    ? "Notifications paused · click to resume"
    : "Pause desktop notifications";
  if (!state.unlocked) {
    clearWorkspaceImages();
    getElement("lock-title").textContent = state.created
      ? "Welcome back."
      : "Your space starts here.";
    getElement("lock-description").textContent = state.created
      ? "Unlock your space to continue the conversation."
      : "Create a password with at least 12 characters to protect your accounts.";
    getElement("confirm-wrap").hidden = state.created;
    getElement("confirm-secret").required = !state.created;
    getElement("methods").hidden = !state.created;
    getElement("unlock-submit").textContent = state.created
      ? "Unlock my space →"
      : "Create my vault →";
    document.querySelector('[data-method="pin"]').hidden = !state.pin;
    document.querySelector('[data-method="key"]').hidden = !state.keys.length;
    getElement("key-choice").replaceChildren(
      ...state.keys.map((k) => {
        const o = createElement("option", "", k.name);
        o.value = k.id;
        return o;
      }),
    );
    if (state.error) {
      getElement("unlock-error").textContent = state.error;
      getElement("unlock-submit").disabled = true;
      getElement("retry-lock").hidden = false;
    }
    return;
  }
  if (!state.organization.workspaces.some((w) => w.id === activeWorkspace))
    activeWorkspace = state.organization.workspaces[0].id;
  if (selected && !state.accounts.some((a) => a.id === selected)) selected = null;
  renderNavigation(state, activeWorkspace, selected, {
    selectWorkspace,
    selectAccount,
    editAccount,
    editCategory,
    addAccount,
  });
  updateAccountStatus(null, selected, state.organization, activeWorkspace);
  getElement("page-title").textContent =
    state.accounts.find((a) => a.id === selected)?.name || "Overview";
  renderChannelIdentity(state.accounts.find((account) => account.id === selected));
  getElement("dashboard").hidden = !!selected;
  getElement("overview-button").classList.toggle("active", !selected);
}
async function selectWorkspace(id) {
  await selectAccount(null);
  activeWorkspace = id;
  selected = null;
  renderState();
}
let selectionQueue = Promise.resolve();
let selectionGeneration = 0;
function selectAccount(id) {
  const generation = ++selectionGeneration;
  selectionQueue = selectionQueue
    .catch(() => {})
    .then(() => {
      if (generation === selectionGeneration) return activateAccount(id);
    });
  return selectionQueue;
}
async function activateAccount(id) {
  if (id && !state.organization.placements[id]) throw Error("Channel not found.");
  if (id && state.organization.placements[id]?.workspaceId !== activeWorkspace) {
    activeWorkspace = state.organization.placements[id].workspaceId;
    renderState();
  }
  if (id) await invoke("sync_accounts", { accountId: id });
  await invoke("select_account", { id });
  if (id) {
    try {
      await request("account.activate", { id });
      state.lastAccount = id;
    } catch {
      showToast("Could not remember the last account. Your session remains open.");
    }
  }
  selected = id;
  if (!id) refreshAvatars();
  if (id) revealChannel(id);
  updateAccountStatus(null, selected);
  getElement("page-title").textContent =
    state.accounts.find((a) => a.id === id)?.name || "Overview";
  renderChannelIdentity(state.accounts.find((account) => account.id === id));
  getElement("dashboard").hidden = !!id;
  getElement("overview-button").classList.toggle("active", !id);
  document
    .querySelectorAll("#account-list [data-account-id]")
    .forEach((account) =>
      account.classList.toggle("selected", account.dataset.accountId === selected),
    );
}
getElement("unlock-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  getElement("unlock-error").textContent = "";
  const submit = getElement("unlock-submit");
  submit.disabled = true;
  let secret = method === "pin" ? digits.map((n) => n.value).join("") : getElement("secret").value;
  try {
    if (!state.created && secret !== getElement("confirm-secret").value)
      throw Error("The passwords do not match.");
    showLoading("Unlocking your space…");
    await request(state.created ? "unlock" : "create", {
      method,
      secret,
      keyId: getElement("key-choice").value,
    });
    clearSecrets();
  } catch (err) {
    hideLoading();
    getElement("unlock-error").textContent = errorMessage(err);
    clearSecrets();
  } finally {
    secret = "";
    submit.disabled = false;
  }
});
getElement("retry-lock").addEventListener("click", () =>
  request("lock").catch((e) => showToast(errorMessage(e))),
);
async function openDialog(title, build, action, submit = "Save") {
  if (dialogOpening || getElement("dialog").open) return;
  dialogOpening = true;
  try {
    await selectionQueue.catch(() => {});
    restoreSelected = selected;
    dialogFocus = document.activeElement;
    await invoke("select_account", { id: null });
    getElement("dialog-title").textContent = title;
    getElement("dialog-content").replaceChildren();
    getElement("dialog-error").textContent = "";
    getElement("dialog-submit").textContent = submit;
    getElement("dialog-submit").hidden = !action;
    getElement("dialog-cancel").textContent = action ? "Cancel" : "Done";
    dialogAction = action;
    getElement("dialog-feedback").hidden = true;
    build(getElement("dialog-content"));
    getElement("dialog").showModal();
    getElement("dialog-content")
      .querySelector("input:not([type=file]), [role=tab], button")
      ?.focus();
  } finally {
    dialogOpening = false;
  }
}

async function closeDialog() {
  const target = restoreSelected;
  restoreSelected = null;
  dialogAction = null;
  getElement("dialog-feedback").hidden = true;
  getElement("dialog").close();
  getElement("dialog-content")
    .querySelectorAll("input")
    .forEach((input) => {
      input.value = "";
    });
  getElement("dialog-content").replaceChildren();
  if (target && state.accounts.some((a) => a.id === target)) await selectAccount(target);
  else if (dialogFocus?.isConnected) dialogFocus.focus();
  else getElement("overview-button").focus();
  dialogFocus = null;
}
const dismissDialog = () => {
  if (!dialogSubmitting && !getElement("dialog").querySelector('[aria-busy="true"]'))
    closeDialog().catch((error) => showToast(errorMessage(error)));
};
getElement("dialog-close").addEventListener("click", dismissDialog);
getElement("dialog-cancel").addEventListener("click", dismissDialog);
getElement("dialog").addEventListener("cancel", (event) => {
  event.preventDefault();
  dismissDialog();
});
getElement("dialog-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  if (!dialogAction || dialogSubmitting || getElement("dialog").querySelector('[aria-busy="true"]'))
    return;
  dialogSubmitting = true;
  getElement("dialog-feedback").hidden = true;
  getElement("dialog-error").textContent = "";
  const submit = getElement("dialog-submit");
  const label = submit.textContent;
  submit.textContent = "Saving…";
  const controls = Array.from(getElement("dialog-form").querySelectorAll("button, input, select"));
  const enabled = controls.filter((control) => !control.disabled);
  enabled.forEach((control) => {
    control.disabled = true;
  });
  try {
    await dialogAction?.();
    await refreshState();
    await closeDialog();
  } catch (err) {
    getElement("dialog-error").textContent = errorMessage(err);
  } finally {
    enabled.forEach((control) => {
      control.disabled = false;
    });
    submit.textContent = label;
    dialogSubmitting = false;
  }
});
async function addAccount(categoryId = null) {
  let name, placement;
  await openDialog(
    "Add WhatsApp",
    (p) => {
      p.append(createElement("p", "", "Name your channel. Then scan the QR code."));
      name = createField(p, "Account name", "text", "", "E.g. Personal, Work…");
      name.maxLength = 40;
      name.required = true;
      const workspace = state.organization.workspaces.find((item) => item.id === activeWorkspace);
      const location = createElement("details", "channel-placement");
      location.open = !!categoryId || workspace.categories.length > 0;
      const locationLabel = createElement("summary", "", `Location · ${workspace.name}`);
      location.append(locationLabel);
      p.append(location);
      placement = placementFields(state, location, activeWorkspace, categoryId);
      location.addEventListener("change", () => {
        const destination = placement();
        const workspace = state.organization.workspaces.find(
          (item) => item.id === destination.workspaceId,
        );
        const category = workspace.categories.find((item) => item.id === destination.categoryId);
        locationLabel.textContent = `Location · ${workspace.name}${category ? ` / ${category.name}` : ""}`;
      });
    },
    async () => {
      const result = await request("account.add", { name: name.value, ...placement() });
      restoreSelected =
        result.accounts.find(
          (account) => !state.accounts.some((existing) => existing.id === account.id),
        )?.id || null;
    },
    "Connect account",
  );
}
async function editAccount(a) {
  let name, placement;
  await openDialog(
    "Manage account",
    (p) => {
      name = createField(p, "Account name", "text", a.name);
      name.maxLength = 40;
      name.required = true;
      const current = state.organization.placements[a.id];
      placement = placementFields(state, p, current.workspaceId, current.categoryId);
      const actions = createElement("div", "dialog-actions");
      const move = async (delta) => {
        const ids = state.accounts.map((a) => a.id);
        const siblings = ids.filter((id) => {
          const p = state.organization.placements[id];
          return p.workspaceId === current.workspaceId && p.categoryId === current.categoryId;
        });
        const target = siblings[siblings.indexOf(a.id) + delta];
        if (!target) return;
        const i = ids.indexOf(a.id),
          j = ids.indexOf(target);
        [ids[i], ids[j]] = [ids[j], ids[i]];
        await request("account.reorder", { ids });
        await refreshState();
        showToast("Order updated.");
      };
      actions.append(
        createButton("↑ Move up", () => move(-1)),
        createButton("↓ Move down", () => move(1)),
      );
      p.append(
        actions,
        createElement(
          "p",
          "",
          "Removing an account deletes its local profile. You can also revoke this device under “Linked devices” on your phone.",
        ),
      );
      p.append(
        createDestructiveAction(
          "Remove this account",
          `Remove “${a.name}” and its local session? This does not delete your WhatsApp account.`,
          async () => {
            await request("account.remove", { id: a.id });
            restoreSelected = null;
            selected = selected === a.id ? null : selected;
            await refreshState();
            await closeDialog();
          },
        ),
      );
    },
    () => request("account.update", { id: a.id, name: name.value, ...placement() }),
  );
}
function notificationControls(parent) {
  const section = createElement("section", "dialog-section");
  section.append(createElement("h3", "", "Notifications"));
  const checkbox = (label, checked) => {
    const wrapper = createElement("label", "toggle-setting");
    const input = createElement("input");
    input.type = "checkbox";
    input.checked = checked;
    wrapper.append(input, document.createTextNode(label));
    section.append(wrapper);
    return input;
  };
  const preferences = state.notifications || { doNotDisturb: false, mutedAccounts: [] };
  const detailLabel = createElement("label", "", "Notification content");
  const detail = createElement("select");
  detail.id = `field-${crypto.randomUUID()}`;
  detailLabel.htmlFor = detail.id;
  for (const [value, label] of [
    ["private", "Private — hide all details"],
    ["channel", "Channel name only"],
    ["preview", "Channel, sender and message"],
  ]) {
    const option = createElement("option", "", label);
    option.value = value;
    detail.append(option);
  }
  detail.value = preferences.detail || "private";
  section.append(
    detailLabel,
    detail,
    createElement(
      "p",
      "hint",
      "Previews use the sender and text provided by WhatsApp. Details are shared with Linux and may remain in notification history or appear on the lock screen, depending on system settings.",
    ),
  );
  const dnd = checkbox("Do not disturb", preferences.doNotDisturb);
  const accounts = state.accounts.map((account) => ({
    id: account.id,
    input: checkbox(`Mute ${account.name}`, preferences.mutedAccounts.includes(account.id)),
  }));
  section.append(
    createElement(
      "p",
      "hint",
      "Muting stops desktop popups. Badges use WhatsApp’s unread counter when available, otherwise received notifications. Keep notifications enabled inside WhatsApp.",
    ),
    createButton("Test Linux notification", async () => {
      await invoke("test_notification");
      showToast(
        "Linux accepted the test notification. If no banner appeared, check GNOME notification settings.",
      );
    }),
    createButton("Save notification settings", async () => {
      await request("notifications", {
        detail: detail.value,
        doNotDisturb: dnd.checked,
        mutedAccounts: accounts
          .filter((account) => account.input.checked)
          .map((account) => account.id),
      });
      await refreshState();
      showToast("Notification settings saved.");
    }),
  );
  parent.append(section);
}

async function showDiagnostics() {
  const report = await invoke("diagnostics");
  // Only an explicit allowlist is displayed; never include controller error strings or account names.
  const safe = {
    ...report,
    vault: state.error ? "Closure requires attention" : state.unlocked ? "Unlocked" : "Locked",
  };
  await openDialog(
    "Safe diagnostics",
    (parent) => {
      parent.append(
        createElement(
          "p",
          "hint",
          "Local report only. No messages, cookies, credentials, account names, paths or remote URLs. Page loaded does not confirm WhatsApp sign-in.",
        ),
      );
      const output = createElement("textarea", "diagnostics-output");
      output.readOnly = true;
      output.setAttribute("aria-label", "Diagnostic report");
      output.value = JSON.stringify(safe, null, 2);
      parent.append(
        output,
        createButton("Select report", () => {
          output.focus();
          output.select();
        }),
      );
    },
    null,
  );
}

async function openSettings() {
  const updatePreferences = await request("update.preferences");
  await openDialog(
    "Settings",
    (p) => {
      const [general, notifications, security, updates] = createTabs(p, [
        "General",
        "Notifications",
        "Security",
        "Updates",
      ]);
      notificationControls(notifications);
      general.append(
        createElement("h3", "", "On this device"),
        createElement(
          "p",
          "",
          "Your workspaces and sessions are stored inside your encrypted vault.",
        ),
      );
      const timeout = createField(
        general,
        "Lock after inactivity (minutes)",
        "number",
        String(state.idleMinutes),
      );
      timeout.min = 0;
      timeout.max = 1440;
      general.append(
        createElement(
          "p",
          "hint",
          "Set to 0 to turn off the inactivity timer. Locking your screen still locks the vault.",
        ),
        createButton("Save lock timeout", async () => {
          if (!timeout.reportValidity()) return;
          await request("settings", { idleMinutes: Number(timeout.value) });
          await refreshState();
          showToast("Lock timeout updated.");
        }),
      );
      const updateEvents = new AbortController();
      getElement("dialog").addEventListener("close", () => updateEvents.abort(), { once: true });
      updateSettings(updates, {
        preferences: updatePreferences,
        version: state.version,
        feedback: showToast,
        signal: updateEvents.signal,
        restart: async () => {
          await closeDialog();
          await restartForUpdate();
        },
      });
      const support = createElement("section", "dialog-section");
      support.append(
        createElement("h3", "", "Troubleshooting"),
        createElement(
          "p",
          "",
          "Check connection and app status without sharing messages or account details.",
        ),
      );
      support.append(
        createButton("Safe diagnostics", async () => {
          await closeDialog();
          await showDiagnostics();
        }),
      );
      general.append(support);
      const sec = createElement("section", "dialog-section");
      sec.append(
        createElement("h3", "", "Unlock methods"),
        createElement("p", "", "Confirm your master password to change your protection settings."),
      );
      const password = createField(sec, "Current master password", "password");
      const pin = createField(sec, "PIN for this computer", "password", "", "6 digits");
      pin.inputMode = "numeric";
      pin.maxLength = 6;
      const actions = createElement("div", "dialog-actions");
      actions.append(
        createButton(state.pin ? "Change PIN" : "Enable PIN", async () => {
          await request("pin", { password: password.value, pin: pin.value });
          pin.value = "";
          password.value = "";
          showToast("PIN saved in this computer’s keyring.");
        }),
        createButton("Disable PIN", async () => {
          await request("pin", { password: password.value, pin: "" });
          pin.value = "";
          password.value = "";
          showToast("PIN disabled.");
        }),
      );
      sec.append(actions);
      const newPassword = createField(
        sec,
        "New master password",
        "password",
        "",
        "At least 12 characters",
      );
      sec.append(
        createButton("Change password", async () => {
          await request("password", { password: password.value, newPassword: newPassword.value });
          password.value = "";
          newPassword.value = "";
          showToast("Password updated.");
        }),
      );
      security.append(sec);
      const keys = createElement("section", "dialog-section");
      keys.append(
        createElement("h3", "", "Security keys"),
        createElement(
          "p",
          "",
          "Use a FIDO2 key with hmac-secret/PRF and user verification. Enrollment may require two touches. Your master password remains available.",
        ),
      );
      const keyName = createField(keys, "Key name", "text", "", "E.g. My YubiKey");
      const keyPin = createField(keys, "Security key PIN", "password");
      keys.append(
        createButton("Register key", async () => {
          showToast("Touch the key when prompted.");
          await request("key.add", {
            password: password.value,
            pin: keyPin.value,
            name: keyName.value,
          });
          password.value = "";
          keyPin.value = "";
          await refreshState();
          showToast("Key registered.");
          await closeDialog();
        }),
      );
      for (const k of state.keys) {
        const row = createElement("div", "key-row");
        row.append(
          createElement("span", "", k.name),
          createButton(
            "Remove",
            async () => {
              await request("key.remove", { password: password.value, id: k.id });
              password.value = "";
              row.remove();
              showToast("Key removed.");
            },
            "text-button",
          ),
        );
        keys.append(row);
      }
      security.append(keys);
    },
    null,
  );
}
async function findChannel() {
  const action = (label, icon, run, hint = "Action") => ({
    label,
    icon,
    hint,
    run: async () => {
      await closeDialog();
      await run();
    },
  });
  await openChannelSearch(
    state,
    openDialog,
    async (id) => {
      restoreSelected = id;
      await closeDialog();
    },
    [
      action("Add WhatsApp", "plus", () => addAccount()),
      action("Create workspace", "grid", () => editWorkspace()),
      action("Settings", "settings", openSettings),
      ...state.organization.workspaces.map((workspace) =>
        action(workspace.name, "folder", () => selectWorkspace(workspace.id), "Workspace"),
      ),
    ],
  );
}
getElement("workspace-quick-search").addEventListener("click", () =>
  findChannel().catch((error) => showToast(errorMessage(error))),
);
getElement("find-channel").addEventListener("click", () =>
  findChannel().catch((error) => showToast(errorMessage(error))),
);
document.addEventListener("keydown", (event) => {
  if (
    state.unlocked &&
    !getElement("dialog").open &&
    (event.ctrlKey || event.metaKey) &&
    event.key.toLowerCase() === "k"
  ) {
    event.preventDefault();
    findChannel().catch((error) => showToast(errorMessage(error)));
  }
});
getElement("retry-account").addEventListener("click", async () => {
  if (!selected) return;
  const button = getElement("retry-account");
  button.disabled = true;
  try {
    await invoke("retry_account", { id: selected });
  } catch (error) {
    showToast(errorMessage(error));
  } finally {
    setTimeout(() => {
      button.disabled = false;
    }, 1000);
  }
});
getElement("add-workspace").addEventListener("click", () =>
  editWorkspace().catch((e) => showToast(errorMessage(e))),
);
getElement("manage-workspace").addEventListener("click", () =>
  editWorkspace(state.organization.workspaces.find((w) => w.id === activeWorkspace)).catch((e) =>
    showToast(errorMessage(e)),
  ),
);
getElement("add-category").addEventListener("click", () =>
  editCategory().catch((e) => showToast(errorMessage(e))),
);
getElement("add-account").addEventListener("click", () =>
  addAccount().catch((e) => showToast(errorMessage(e))),
);
getElement("add-account-card").addEventListener("click", () =>
  addAccount().catch((e) => showToast(errorMessage(e))),
);
getElement("quiet-button").addEventListener("click", async () => {
  const button = getElement("quiet-button");
  if (button.disabled) return;
  button.disabled = true;
  try {
    const preferences = state.notifications || { detail: "private", mutedAccounts: [] };
    await request("notifications", { ...preferences, doNotDisturb: !preferences.doNotDisturb });
    await refreshState();
    showToast(
      state.notifications.doNotDisturb
        ? "Desktop notifications paused. Unread badges remain active."
        : "Desktop notifications resumed.",
    );
  } catch (error) {
    showToast(errorMessage(error));
  } finally {
    button.disabled = false;
  }
});
getElement("settings-button").addEventListener("click", () =>
  openSettings().catch((e) => showToast(errorMessage(e))),
);
getElement("lock-button").addEventListener("click", () =>
  request("lock").catch((e) => showToast(errorMessage(e))),
);
getElement("overview-button").addEventListener("click", () => selectAccount(null));
document.querySelector(".home-button").addEventListener("click", () => selectAccount(null));
let activityAt = 0;
for (const event of ["pointerdown", "keydown"])
  document.addEventListener(event, () => {
    if (state.unlocked && Date.now() - activityAt > 5000) {
      activityAt = Date.now();
      request("activity").catch(() => {});
    }
  });
async function initializeDesktop() {
  await Promise.all([
    listen("account-avatar", (event) => updateAvatar(event.payload)),
    listen("account-state", (event) => updateAccountStatus(event.payload, selected)),
    listen("account-error", (event) => showToast(event.payload)),
    listen("open-account", async (event) => {
      if (startingAccounts) {
        pendingOpenAccount = event.payload;
        return;
      }
      if (getElement("dialog").open) await closeDialog();
      await selectAccount(event.payload);
    }),
  ]);
  showLoading("Opening Elo Talks…");
  state = await request("status");
  if (!(await initializeUpdates(state.unlocked))) return;
  renderState();
  document.body.classList.remove("booting");
  if (state.unlocked) {
    startingAccounts = true;
    try {
      await prepareAccounts(state.accounts, state.lastAccount, selectAccount);
    } finally {
      startingAccounts = false;
    }
    if (pendingOpenAccount) await selectAccount(pendingOpenAccount);
  } else if ((await request("update.status")).phase !== "installing") {
    hideLoading();
  }
}
if (isDesktop) {
  initializeDesktop().catch((error) => {
    document.body.classList.remove("booting");
    hideLoading();
    getElement("unlock-error").textContent = errorMessage(error);
  });
} else {
  document.body.classList.remove("booting");
  renderState();
  getElement("unlock-error").textContent =
    "Visual preview. The vault is only available in the desktop application.";
}

for (const control of document.querySelectorAll("[data-window-action]")) {
  control.addEventListener("click", () => {
    invoke("window_action", { action: control.dataset.windowAction }).catch((error) =>
      showToast(errorMessage(error)),
    );
  });
}
getElement("window-drag").addEventListener("mousedown", (event) => {
  if (event.button !== 0) return;
  invoke("window_action", { action: event.detail === 2 ? "maximize" : "drag" }).catch(() => {});
});
getElement("app-menu").addEventListener("click", () => {
  openDialog("Elo Talks", (content) => {
    const actions = createElement("div", "application-menu-actions");
    const add = (label, action) =>
      actions.append(
        createButton(label, async () => {
          await closeDialog();
          await action();
        }),
      );
    if (state.unlocked) {
      add("Overview", () => selectAccount(null));
      add("Settings", openSettings);
      add("Lock vault", () => request("lock"));
    }
    add("Minimize", () => invoke("window_action", { action: "minimize" }));
    add("Close window", () => invoke("window_action", { action: "close" }));
    add("Quit Elo Talks", () => request("quit"));
    content.append(actions);
  }).catch((error) => showToast(errorMessage(error)));
});

getElement("channel-settings").addEventListener("click", () => {
  const account = state.accounts.find((account) => account.id === selected);
  if (account) editAccount(account).catch((error) => showToast(errorMessage(error)));
});
