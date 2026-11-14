/** Small platform gaps in Android 9's factory WebView. Load before Tauri's API
 * and any DOM components; no domain or scheduling rules belong in this module. */
if (typeof globalThis === "undefined") {
  Object.defineProperty(window, "globalThis", {
    value: window,
    writable: true,
    configurable: true,
  });
}
if (!Element.prototype.replaceChildren) {
  Element.prototype.replaceChildren = function (...nodes: (Node | string)[]) {
    this.textContent = "";
    this.append(...nodes);
  };
}
