/** Preserve a control's identity when a view is rerendered. Dialog contents live
 * outside that view so async updates cannot discard an unfinished address form. */
export function retainFocus(root: HTMLElement): () => void {
  const active = document.activeElement;
  if (!(active instanceof HTMLElement) || !root.contains(active))
    return () => {};
  const id = active.id;
  const keys = [
    "data-action",
    "data-page",
    "data-day",
    "data-mode",
    "data-theme",
    "data-font",
    "data-id",
    "data-minutes",
    "data-interval",
  ];
  const attributes = keys
    .filter((key) => active.hasAttribute(key))
    .map((key) => [key, active.getAttribute(key)] as const);
  return () => {
    const target = [
      ...root.querySelectorAll<HTMLElement>(
        "button, input, select, [tabindex]",
      ),
    ].find((node) =>
      id
        ? node.id === id
        : attributes.length > 0 &&
          attributes.every(([key, value]) => node.getAttribute(key) === value),
    );
    if (target && !target.hasAttribute("disabled"))
      target.focus({ preventScroll: true });
  };
}
