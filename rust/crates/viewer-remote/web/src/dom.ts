type Child = Node | string | null | undefined | false;
type Attrs = Record<string, string | boolean | ((event: Event) => void) | undefined>;

export function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Attrs = {},
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  for (const [name, value] of Object.entries(attrs)) {
    if (value === undefined || value === false) continue;
    if (typeof value === "function") node.addEventListener(name.slice(2).toLowerCase(), value);
    else if (value === true) node.setAttribute(name, "");
    else node.setAttribute(name, value);
  }
  for (const child of children) if (child) node.append(child);
  return node;
}

const ICONS: Record<string, string> = {
  play: "M8 5v14l11-7z",
  pause: "M6 5h4v14H6zM14 5h4v14h-4z",
  stop: "M6 6h12v12H6z",
  back10: "M11 7V3L5 9l6 6v-4c3.3 0 6 2.7 6 6 0 .3 0 .7-.1 1A8 8 0 0 0 11 7z",
  forward10: "M13 7V3l6 6-6 6v-4c-3.3 0-6 2.7-6 6 0 .3 0 .7.1 1A8 8 0 0 1 13 7z",
  live: "M4 6l7 6-7 6zM12 6l7 6-7 6z",
  volume: "M4 9v6h4l5 4V5L8 9zM16 8.5a5 5 0 0 1 0 7M18.5 6a8.5 8.5 0 0 1 0 12",
  muted: "M4 9v6h4l5 4V5L8 9zM16 9l5 6M21 9l-5 6",
  subtitles: "M3 6h18v12H3zM6 13h5M13 13h5M6 16h8",
  up: "M6 15l6-6 6 6",
  down: "M6 9l6 6 6-6",
  left: "M15 6l-6 6 6 6",
  right: "M9 6l6 6-6 6",
  back: "M9 8L5 12l4 4M5 12h10a4 4 0 0 1 0 8h-3",
  guide: "M3 5h18v14H3zM3 10h18M9 10v9",
  list: "M4 6h16M4 12h16M4 18h16",
  fullscreen: "M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5",
  last: "M7 7h10v10M17 7L7 17",
  remote: "M8 2h8a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2zM12 6v.01M9 11h6M12 8v6",
};

const FILLED = new Set(["play", "pause", "stop", "back10", "forward10", "live"]);

export function icon(name: keyof typeof ICONS | string): SVGSVGElement {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  svg.classList.add("icon", FILLED.has(name) ? "filled" : "stroked");
  const path = document.createElementNS("http://www.w3.org/2000/svg", "path");
  path.setAttribute("d", ICONS[name] ?? "");
  svg.append(path);
  return svg;
}

export function tap(): void {
  // Short haptic confirmation where the browser supports it (not iOS Safari).
  navigator.vibrate?.(8);
}
