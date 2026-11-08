import { invoke } from "./bridge.js";
import { createElement, createButton, errorMessage } from "./dom.js";

export function mediaPermissionControls(container, accounts, feedback) {
  const section = createElement("section", "dialog-section");
  section.append(
    createElement("h3", "", "Microphone and camera"),
    createElement(
      "p",
      "hint",
      "Permissions apply only to web.whatsapp.com in each account and are saved inside your vault. Changing access reloads that account and ends any active call.",
    ),
  );
  container.append(section);
  const content = createElement("div");
  section.append(content);
  async function render() {
    const permissions = await invoke("media_permissions");
    content.replaceChildren();
    for (const account of accounts) {
      const saved = permissions[account.id];
      if (!saved) continue;
      content.append(createElement("h4", "", account.name));
      for (const device of ["microphone", "camera"]) {
        const value = saved[device];
        const row = createElement("div", "dialog-section");
        const label = device === "microphone" ? "Microphone" : "Camera";
        row.append(
          createElement(
            "p",
            "",
            `${label}: ${{ ask: "Ask next time", allow: "Allowed", block: "Blocked" }[value]}`,
          ),
        );
        for (const [text, block] of [
          [`Revoke ${device}`, true],
          [`Ask for ${device} again`, false],
        ]) {
          const button = createButton(text, async () => {
            button.disabled = true;
            try {
              await invoke("reset_media_permission", { id: account.id, device, block });
              await render();
              feedback("Device permission updated. The account is reloading.");
            } catch (error) {
              feedback(errorMessage(error));
              button.disabled = false;
            }
          });
          button.disabled = block ? value === "block" : value === "ask";
          row.append(button);
        }
        content.append(row);
      }
    }
    if (!content.childElementCount)
      content.append(
        createElement("p", "hint", "Open an account to manage its device permissions."),
      );
  }
  render().catch((error) => {
    content.textContent = errorMessage(error);
  });
}
