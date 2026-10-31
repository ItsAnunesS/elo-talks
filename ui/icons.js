// Small local vectors; no icon font, external request or runtime dependency.
const paths = {
  plus: "M12 5v14M5 12h14",
  bell: "M18 8a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9M10 21h4",
  close: "m6 6 12 12M18 6 6 18",
  search: "M21 21l-5-5M18 10a8 8 0 1 1-16 0 8 8 0 0 1 16 0",
  chevron: "m8 10 4 4 4-4",
  grid: "M3 3h7v7H3zM14 3h7v7h-7zM3 14h7v7H3zM14 14h7v7h-7z",
  lock: "M5 10h14v11H5zM8 10V6a4 4 0 0 1 8 0v4",
  shield: "m12 2 8 3v6c0 5-5 9-8 11-3-2-8-6-8-11V5zM8 12l3 3 5-6",
  settings:
    "m9 3-1 3-3 1-2 3 2 2-1 3 2 3 3-1 3 2 3-2 3 1 2-3-1-3 2-2-2-3-3-1-1-3zM15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0",
  arrow: "M5 12h14m-5-5 5 5-5 5",
  folder: "M3 5h6l2 2h10v13H3z",
};
export function icon(name) {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("class", "ui-icon");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "1.6");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  const path = document.createElementNS(svg.namespaceURI, "path");
  path.setAttribute("d", paths[name] || paths.grid);
  svg.append(path);
  return svg;
}
export function hydrateIcons() {
  document
    .querySelectorAll("[data-icon]")
    .forEach((element) => element.replaceChildren(icon(element.dataset.icon)));
}
