import { createButton, createElement } from "./dom.js";
let menu;
let dismiss;
export function openContextMenu(event, actions) {
  event.preventDefault();
  dismiss?.();
  menu = createElement("div", "context-menu");
  menu.setAttribute("role", "menu");
  const origin = document.activeElement;
  const current = menu;
  const close = (restore = false) => {
    current.remove();
    controller.abort();
    if (restore && origin?.isConnected) origin.focus();
  };
  dismiss = close;
  const controller = new AbortController();
  for (const { label, run } of actions) {
    const button = createButton(
      label,
      async () => {
        close();
        await run();
      },
      "context-action",
    );
    button.setAttribute("role", "menuitem");
    current.append(button);
  }
  document.body.append(current);
  current.style.left = `${Math.max(4, Math.min(event.clientX || 80, (event.clientX < 304 ? 304 : innerWidth) - current.offsetWidth - 8))}px`;
  current.style.top = `${Math.max(40, Math.min(event.clientY || 80, innerHeight - current.offsetHeight - 8))}px`;
  current.firstElementChild?.focus();
  current.addEventListener("keydown", (event) => {
    const rows = [...current.children];
    const index = rows.indexOf(document.activeElement);
    if (event.key === "Escape" || event.key === "Tab") {
      event.preventDefault();
      close(true);
    } else if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
      event.preventDefault();
      const next =
        event.key === "Home"
          ? 0
          : event.key === "End"
            ? rows.length - 1
            : (index + (event.key === "ArrowDown" ? 1 : rows.length - 1)) % rows.length;
      rows[next]?.focus();
    }
  });
  document.addEventListener(
    "pointerdown",
    (event) => {
      if (!current.contains(event.target)) close();
    },
    { signal: controller.signal },
  );
  window.addEventListener("blur", () => close(), { signal: controller.signal, once: true });
}
