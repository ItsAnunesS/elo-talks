import { getElement, errorMessage, createElement, createButton, createField } from "./dom.js";
import { invoke, request, listen, isDesktop } from "./bridge.js";
import { initializePinInputs } from "./pin-input.js";
const digits = initializePinInputs(getElement("pin-inputs"));
let state = { created: false, unlocked: false, pin: false, keys: [], accounts: [], idleMinutes: 0 };
let method = "password",
  selected = null,
  restoreSelected = null,
  dialogAction;
function showToast(text) {
  getElement("toast").textContent = text;
  getElement("toast").hidden = false;
  setTimeout(() => {
    getElement("toast").hidden = true;
  }, 5000);
}
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
  if (state.unlocked) await invoke("sync_accounts");
}
function renderState() {
  getElement("lock-screen").hidden = state.unlocked;
  getElement("workspace").hidden = !state.unlocked;
  getElement("version").textContent = `v${state.version || "0.1.0"}`;
  if (!state.unlocked) {
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
  getElement("account-icons").replaceChildren();
  getElement("account-list").replaceChildren();
  getElement("cards").replaceChildren();
  getElement("account-count").textContent = state.accounts.length;
  for (const a of state.accounts) {
    const initial = a.name.slice(0, 2).toUpperCase();
    const icon = createButton(initial, () => selectAccount(a.id), "account-icon");
    icon.dataset.accountId = a.id;
    icon.title = a.name;
    icon.setAttribute("aria-label", a.name);
    icon.classList.toggle("selected", selected === a.id);
    getElement("account-icons").append(icon);
    const row = createElement("div", "account-row");
    row.dataset.accountId = a.id;
    row.classList.toggle("selected", selected === a.id);
    row.append(
      createButton(a.name, () => selectAccount(a.id), "account-name"),
      createButton("⋯", () => editAccount(a), "account-menu"),
    );
    getElement("account-list").append(row);
    const card = createButton("", () => selectAccount(a.id), "account-card");
    card.append(
      createElement("span", "card-avatar", initial),
      createElement("strong", "", a.name),
      createElement("small", "", "Open WhatsApp →"),
    );
    getElement("cards").append(card);
  }
}
async function selectAccount(id) {
  await invoke("select_account", { id });
  selected = id;
  getElement("page-title").textContent =
    state.accounts.find((a) => a.id === id)?.name || "Overview";
  getElement("dashboard").hidden = !!id;
  getElement("overview-button").classList.toggle("active", !id);
  for (const container of ["account-icons", "account-list"]) {
    for (const account of getElement(container).children) {
      account.classList.toggle("selected", account.dataset.accountId === selected);
    }
  }
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
    await request(state.created ? "unlock" : "create", {
      method,
      secret,
      keyId: getElement("key-choice").value,
    });
    clearSecrets();
  } catch (err) {
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
  restoreSelected = selected;
  await invoke("select_account", { id: null });
  getElement("dialog-title").textContent = title;
  getElement("dialog-content").replaceChildren();
  getElement("dialog-error").textContent = "";
  getElement("dialog-submit").textContent = submit;
  getElement("dialog-submit").hidden = !action;
  dialogAction = action;
  build(getElement("dialog-content"));
  getElement("dialog").showModal();
}
async function closeDialog() {
  getElement("dialog").close();
  getElement("dialog-content")
    .querySelectorAll("input")
    .forEach((i) => (i.value = ""));
  if (restoreSelected) await selectAccount(restoreSelected);
  restoreSelected = null;
}
getElement("dialog-close").addEventListener("click", () =>
  closeDialog().catch((e) => showToast(errorMessage(e))),
);
getElement("dialog").addEventListener("cancel", (e) => {
  e.preventDefault();
  closeDialog();
});
getElement("dialog-form").addEventListener("submit", async (e) => {
  e.preventDefault();
  getElement("dialog-error").textContent = "";
  getElement("dialog-submit").disabled = true;
  try {
    await dialogAction?.();
    await closeDialog();
    await refreshState();
  } catch (err) {
    getElement("dialog-error").textContent = errorMessage(err);
  } finally {
    getElement("dialog-submit").disabled = false;
  }
});
async function addAccount() {
  let name;
  await openDialog(
    "Add WhatsApp",
    (p) => {
      p.append(
        createElement("p", "", "Give this account a name. Then scan the QR code with your phone."),
      );
      name = createField(p, "Account name", "text", "", "E.g. Personal, Work…");
      name.maxLength = 40;
      name.required = true;
    },
    async () => {
      await request("account.add", { name: name.value });
    },
    "Connect account →",
  );
}
async function editAccount(a) {
  let name;
  await openDialog(
    "Manage account",
    (p) => {
      name = createField(p, "Account name", "text", a.name);
      name.maxLength = 40;
      name.required = true;
      const actions = createElement("div", "dialog-actions");
      const move = async (delta) => {
        const ids = state.accounts.map((a) => a.id);
        const i = ids.indexOf(a.id),
          j = i + delta;
        if (j < 0 || j >= ids.length) return;
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
        createButton(
          "Remove this account",
          async () => {
            if (!confirm(`Remove account ${a.name} from this computer?`)) return;
            restoreSelected = null;
            await request("account.remove", { id: a.id });
          },
          "secondary danger",
        ),
      );
    },
    () => request("account.rename", { id: a.id, name: name.value }),
  );
}
async function openSettings() {
  await openDialog(
    "Your vault, your rules",
    (p) => {
      const timeout = createField(
        p,
        "Lock after inactivity (minutes; 0 = disabled)",
        "number",
        String(state.idleMinutes),
      );
      timeout.min = 0;
      timeout.max = 1440;
      p.append(
        createButton("Save lock timeout", async () => {
          await request("settings", { idleMinutes: Number(timeout.value) });
          showToast("Lock timeout updated.");
        }),
      );
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
      p.append(sec);
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
      p.append(keys);
    },
    null,
  );
}
getElement("add-account").addEventListener("click", () =>
  addAccount().catch((e) => showToast(errorMessage(e))),
);
getElement("add-account-card").addEventListener("click", () =>
  addAccount().catch((e) => showToast(errorMessage(e))),
);
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
if (isDesktop) {
  listen("account-error", (e) => showToast(e.payload));
  listen("open-account", async (e) => {
    if (getElement("dialog").open) await closeDialog();
    await selectAccount(e.payload);
  });
  refreshState().catch((e) => {
    getElement("unlock-error").textContent = errorMessage(e);
  });
} else {
  renderState();
  getElement("unlock-error").textContent =
    "Visual preview. The vault is only available in the desktop application.";
}
