import type { CatalogEntry } from "../types";
import { uk } from "../strings";

function searchKey(value: string): string {
  return value
    .normalize("NFKC")
    .toLocaleLowerCase("uk-UA")
    .replace(/[’ʼ`]/g, "'")
    .trim();
}

/** A typed query is not a selection. Only a catalog option supplies a stable ID. */
export class Autocomplete {
  readonly element = document.createElement("div");
  readonly input = document.createElement("input");
  private readonly list = document.createElement("ul");
  private options: CatalogEntry[] = [];
  private matches: CatalogEntry[] = [];
  private selected: CatalogEntry | null = null;
  private active = -1;
  private remoteOptions = false;
  private pending = false;

  constructor(
    id: string,
    label: string,
    placeholder: string,
    private readonly onChange: (entry: CatalogEntry | null) => void,
  ) {
    this.element.className = "autocomplete-field";
    const caption = document.createElement("label");
    caption.htmlFor = id;
    caption.textContent = label;
    this.input.id = id;
    this.input.placeholder = placeholder;
    this.input.autocomplete = "off";
    this.input.setAttribute("role", "combobox");
    this.input.setAttribute("aria-autocomplete", "list");
    this.input.setAttribute("aria-expanded", "false");
    this.input.setAttribute("aria-controls", `${id}-options`);
    this.input.setAttribute("aria-describedby", "address-help");
    this.list.id = `${id}-options`;
    this.list.className = "autocomplete-options";
    this.list.setAttribute("role", "listbox");
    this.list.setAttribute("aria-label", label);
    this.list.hidden = true;
    this.element.append(caption, this.input, this.list);
    this.input.addEventListener("input", () => {
      this.selected = null;
      this.remoteOptions = false;
      this.onChange(null);
      this.active = -1;
      this.show();
    });
    this.input.addEventListener("focus", () => this.show());
    this.input.addEventListener("blur", () => this.close());
    this.input.addEventListener("keydown", (event) => this.keydown(event));
    this.list.addEventListener("pointerdown", (event) =>
      event.preventDefault(),
    );
    this.list.addEventListener("click", (event) => {
      const option = (event.target as HTMLElement).closest<HTMLElement>(
        "[data-option]",
      );
      const entry = this.options.find(
        (item) => item.id === option?.dataset.option,
      );
      if (entry) {
        this.select(entry.id);
        this.onChange(entry);
      }
    });
  }

  get value(): CatalogEntry | null {
    return this.selected;
  }

  setOptions(options: CatalogEntry[], disabled = false): void {
    this.remoteOptions = false;
    this.options = options;
    this.selected = null;
    this.input.value = "";
    this.input.disabled = disabled;
    this.pending = false;
    this.input.setAttribute("aria-busy", "false");
    this.active = -1;
    this.close();
  }

  select(id: string): void {
    const entry = this.options.find((item) => item.id === id);
    this.selected = entry ?? null;
    this.input.value = entry?.name ?? "";
    this.close();
  }
  /** Remote suggestions never replace what the user is currently typing. */
  setSuggestions(options: CatalogEntry[]): void {
    this.remoteOptions = true;
    this.options = options;
    this.selected =
      (this.selected &&
        options.find((entry) => entry.id === this.selected?.id)) ||
      null;
    this.active = -1;
    if (document.activeElement === this.input) this.show();
  }

  setLoading(pending: boolean): void {
    this.pending = pending;
    this.input.setAttribute("aria-busy", String(pending));
    if (document.activeElement === this.input) this.show();
  }

  private show(): void {
    if (this.input.disabled) return;
    const key = this.selected ? "" : searchKey(this.input.value);
    this.matches = this.options
      .filter(
        (entry) => this.remoteOptions || searchKey(entry.name).includes(key),
      )
      .sort((a, b) => {
        if (!key || this.remoteOptions) return 0;
        const rank = (entry: CatalogEntry) => {
          const name = searchKey(entry.name.split(" · ")[0]!);
          return name === key
            ? 0
            : name.startsWith(key)
              ? 1
              : name.includes(key)
                ? 2
                : 3;
        };
        return rank(a) - rank(b);
      })
      .slice(0, 60);
    this.list.replaceChildren();
    if (!this.matches.length) {
      const message = document.createElement("li");
      message.className = "autocomplete-empty";
      message.textContent = this.pending ? uk.searchingAddresses : uk.noMatches;
      if (this.pending) message.classList.add("is-pending");
      message.setAttribute("role", "presentation");
      this.list.append(message);
    }
    for (const [index, entry] of this.matches.entries()) {
      const option = document.createElement("li");
      option.id = `${this.input.id}-option-${index}`;
      option.dataset.option = entry.id;
      option.setAttribute("role", "option");
      option.textContent = entry.name;
      option.setAttribute("aria-selected", String(index === this.active));
      this.list.append(option);
    }
    this.list.hidden = false;
    this.input.setAttribute("aria-expanded", "true");
    this.updateActive();
  }

  private close(): void {
    this.list.hidden = true;
    this.input.setAttribute("aria-expanded", "false");
    this.input.removeAttribute("aria-activedescendant");
  }

  private updateActive(): void {
    this.list
      .querySelectorAll<HTMLElement>('[role="option"]')
      .forEach((node, index) => {
        const active = index === this.active;
        node.setAttribute("aria-selected", String(active));
        if (active) {
          this.input.setAttribute("aria-activedescendant", node.id);
          node.scrollIntoView({ block: "nearest" });
        }
      });
    if (this.active < 0) this.input.removeAttribute("aria-activedescendant");
  }

  private keydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && !this.list.hidden) {
      event.preventDefault();
      event.stopPropagation();
      this.close();
      return;
    }
    if (event.key === "Tab") {
      this.close();
      return;
    }
    if (event.key === "Enter" && !this.list.hidden) {
      event.preventDefault();
      const entry = this.matches[this.active];
      if (entry) {
        this.select(entry.id);
        this.onChange(entry);
      }
      return;
    }
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    if (this.list.hidden) {
      this.active = -1;
      this.show();
    }
    if (!this.matches.length) return;
    this.active =
      event.key === "ArrowDown"
        ? (this.active + 1) % this.matches.length
        : this.active <= 0
          ? this.matches.length - 1
          : this.active - 1;
    this.updateActive();
  }
}
