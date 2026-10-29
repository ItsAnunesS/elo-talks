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
    try {
      await onClick();
    } catch (error) {
      getElement("dialog-error").textContent = errorMessage(error);
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
