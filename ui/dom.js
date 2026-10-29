export const getElement = (id) => document.getElementById(id);

export function errorMessage(error) {
  return typeof error === "string" ? error : error.message;
}

export function createElement(tag, className, text) {
  const element = document.createElement(tag);
  if (className) element.className = className;
  if (text !== undefined) element.textContent = text;
  return element;
}

export function createButton(text, onClick, className = "secondary") {
  const button = createElement("button", className, text);
  button.type = "button";
  button.addEventListener("click", async () => {
    if (button.disabled) return;
    const dialog = button.closest("dialog");
    if (dialog?.querySelector('[aria-busy="true"]')) return;
    const footer = dialog
      ? Array.from(dialog.querySelectorAll(".dialog-footer button, #dialog-close")).filter(
          (control) => !control.disabled,
        )
      : [];
    footer.forEach((control) => {
      control.disabled = true;
    });
    if (dialog) {
      getElement("dialog-feedback").hidden = true;
      getElement("dialog-error").textContent = "";
    }
    button.disabled = true;
    button.setAttribute("aria-busy", "true");
    try {
      await onClick();
    } catch (error) {
      const dialog = getElement("dialog");
      if (dialog.open) getElement("dialog-error").textContent = errorMessage(error);
      else window.dispatchEvent(new CustomEvent("ui-error", { detail: errorMessage(error) }));
    } finally {
      button.disabled = false;
      button.removeAttribute("aria-busy");
      footer.forEach((control) => {
        control.disabled = false;
      });
    }
  });
  return button;
}

export function createField(parent, label, type = "text", value = "", placeholder = "") {
  const id = `field-${crypto.randomUUID()}`;
  const caption = createElement("label", "", label);
  caption.htmlFor = id;
  const input = createElement("input");
  Object.assign(input, { id, type, autocomplete: "off", value, placeholder });
  parent.append(caption, input);
  return input;
}

export function createDestructiveAction(label, description, action) {
  const section = createElement("section", "danger-zone");
  const trigger = createButton(
    label,
    () => {
      confirmation.hidden = false;
      trigger.hidden = true;
      cancel.focus();
    },
    "text-button danger",
  );
  const confirmation = createElement("div", "delete-confirmation");
  confirmation.hidden = true;
  confirmation.append(createElement("p", "", description));
  const actions = createElement("div", "dialog-actions");
  const cancel = createButton("Keep it", () => {
    confirmation.hidden = true;
    trigger.hidden = false;
    trigger.focus();
  });
  const confirm = createButton(`Confirm ${label.toLowerCase()}`, action, "secondary danger");
  actions.append(cancel, confirm);
  confirmation.append(actions);
  section.append(trigger, confirmation);
  return section;
}

export function createTabs(parent, names) {
  const tabs = createElement("div", "settings-tabs");
  tabs.setAttribute("role", "tablist");
  tabs.setAttribute("aria-label", "Settings sections");
  parent.append(tabs);
  const panels = names.map((name) => {
    const panel = createElement("section", "settings-panel");
    panel.id = `panel-${crypto.randomUUID()}`;
    panel.setAttribute("role", "tabpanel");
    const button = createButton(name, () => activate(names.indexOf(name)), "settings-tab");
    button.id = `tab-${crypto.randomUUID()}`;
    button.setAttribute("role", "tab");
    button.setAttribute("aria-controls", panel.id);
    panel.setAttribute("aria-labelledby", button.id);
    tabs.append(button);
    parent.append(panel);
    return panel;
  });
  function activate(index, focus = false) {
    getElement("dialog-error").textContent = "";
    parent.scrollTop = 0;
    panels.forEach((panel, i) => {
      panel.hidden = i !== index;
      tabs.children[i].setAttribute("aria-selected", String(i === index));
      tabs.children[i].tabIndex = i === index ? 0 : -1;
      if (panel.hidden)
        panel.querySelectorAll('input[type="password"]').forEach((input) => {
          input.value = "";
        });
    });
    if (focus) tabs.children[index].focus();
  }
  tabs.addEventListener("keydown", (event) => {
    const index = Array.from(tabs.children).indexOf(document.activeElement);
    const keys = {
      ArrowRight: (index + 1) % names.length,
      ArrowLeft: (index + names.length - 1) % names.length,
      Home: 0,
      End: names.length - 1,
    };
    if (event.key in keys) {
      event.preventDefault();
      activate(keys[event.key], true);
    }
  });
  activate(0);
  return panels;
}
