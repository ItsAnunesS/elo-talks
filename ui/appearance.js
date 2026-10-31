import { createElement } from "./dom.js";
const key = "elo-appearance";
function read() {
  try {
    const value = JSON.parse(localStorage.getItem(key) || "{}");
    return {
      density: value.density === "compact" ? "compact" : "comfortable",
      motion: value.motion !== false,
    };
  } catch {
    return { density: "comfortable", motion: true };
  }
}
function apply(value) {
  document.documentElement.dataset.density = value.density;
  document.documentElement.dataset.motion = value.motion ? "system" : "off";
}
apply(read());
export function appearanceControls(parent) {
  const section = createElement("section", "dialog-section");
  section.append(createElement("h3", "", "Appearance"));
  const label = createElement("label", "", "Layout density");
  const density = createElement("select");
  density.id = "appearance-density";
  label.htmlFor = density.id;
  for (const value of ["comfortable", "compact"]) {
    const option = createElement("option", "", value === "compact" ? "Compact" : "Comfortable");
    option.value = value;
    density.append(option);
  }
  const current = read();
  density.value = current.density;
  const toggle = createElement("label", "toggle-setting");
  const motion = createElement("input");
  motion.type = "checkbox";
  motion.checked = current.motion;
  toggle.append(motion, createElement("span", "", "Interface animations"));
  const feedback = createElement("p", "hint");
  feedback.setAttribute("role", "status");
  const save = () => {
    const value = { density: density.value, motion: motion.checked };
    apply(value);
    try {
      localStorage.setItem(key, JSON.stringify(value));
      feedback.textContent = "Saved.";
    } catch {
      feedback.textContent = "Applied for this session. Could not save preferences.";
    }
  };
  density.addEventListener("change", save);
  motion.addEventListener("change", save);
  section.append(
    label,
    density,
    toggle,
    createElement("p", "hint", "Your system’s reduced-motion preference always takes priority."),
    feedback,
  );
  parent.append(section);
}
