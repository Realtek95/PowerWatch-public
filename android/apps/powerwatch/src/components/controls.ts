import { escapeHtml as html, formatDate } from "../format";
import { icon } from "../icons";
import { uk } from "../strings";

/** Native form inputs retain keyboard behavior, validation and FormData. */
export function toggle(
  label: string,
  checked: boolean,
  attributes: string,
  hint = "",
): string {
  return `<label class="switch-row"><span class="switch-copy"><span>${label}</span>${hint ? `<small>${hint}</small>` : ""}</span><input type="checkbox" role="switch" ${attributes} ${checked ? "checked" : ""}><span class="switch-track" aria-hidden="true"></span></label>`;
}

export function frequencyChoice(value: number): string {
  return `<div class="segmented-choice">${[15, 30, 60].map((minutes) => `<label><input type="radio" name="interval" value="${minutes}" ${minutes === value ? "checked" : ""}><span>${minutes} ${uk.minute}</span></label>`).join("")}</div>`;
}

export function selectControl(
  id: string,
  options: { value: string; label: string }[],
  value: string,
  disabled = false,
): string {
  const label =
    options.find((option) => option.value === value)?.label ??
    options[0]?.label ??
    "—";
  return `<div class="select-control"><button type="button" id="${id}" class="select-trigger" value="${html(value)}" aria-haspopup="listbox" aria-expanded="false" aria-controls="${id}-options" ${disabled ? "disabled" : ""}><span>${html(label)}</span>${icon("chevron")}</button><div id="${id}-options" class="select-options" role="listbox" aria-labelledby="${id}" hidden>${options.map((option) => `<button type="button" role="option" tabindex="-1" data-value="${html(option.value)}" aria-selected="${option.value === value}"><span>${html(option.label)}</span>${icon("check")}</button>`).join("")}</div></div>`;
}

export function dateControl(value: string, fallback: string): string {
  return `<div class="date-control" data-month="${(value || fallback).slice(0, 7)}"><button type="button" id="change-date" class="select-trigger" value="${html(value)}" aria-haspopup="dialog" aria-expanded="false" aria-controls="change-calendar">${icon("calendar")}<span>${value ? html(formatDate(value)) : uk.allDates}</span>${icon("chevron")}</button><div class="calendar-popup" id="change-calendar" role="dialog" aria-label="${uk.filterDate}" hidden></div></div>`;
}

const dateKey = (date: Date) => date.toISOString().slice(0, 10);
function calendarMarkup(month: string, selected: string): string {
  const first = new Date(`${month}-01T12:00:00Z`);
  const offset = (first.getUTCDay() + 6) % 7;
  const count = new Date(
    Date.UTC(first.getUTCFullYear(), first.getUTCMonth() + 1, 0),
  ).getUTCDate();
  return `<div class="calendar-heading"><button type="button" class="carousel-arrow" data-month-step="-1" aria-label="${uk.previousMonth}">${icon("chevron", "point-left")}</button><strong aria-live="polite">${new Intl.DateTimeFormat("uk-UA", { month: "long", year: "numeric", timeZone: "UTC" }).format(first)}</strong><button type="button" class="carousel-arrow" data-month-step="1" aria-label="${uk.nextMonth}">${icon("chevron")}</button></div><div class="calendar-week">${uk.weekdays.map((day) => `<span>${day}</span>`).join("")}</div><div class="calendar-grid">${"<span></span>".repeat(offset)}${Array.from(
    { length: count },
    (_, i) => {
      const key = `${month}-${String(i + 1).padStart(2, "0")}`;
      return `<button type="button" data-date="${key}" tabindex="${key === selected || (!selected.startsWith(month) && i === 0) ? "0" : "-1"}" aria-label="${html(formatDate(key))}" aria-pressed="${selected === key}">${i + 1}</button>`;
    },
  ).join(
    "",
  )}</div><button type="button" class="text-button" data-clear-date>${uk.allDates}</button>`;
}

/** Delegation works with retained forms and rerendered pages, including WebView 69. */
export function installControls(root: HTMLElement): void {
  function close(except?: Element) {
    root
      .querySelectorAll<HTMLElement>(".select-control, .date-control")
      .forEach((control) => {
        if (control === except) return;
        control.querySelector<HTMLElement>(
          ".select-options, .calendar-popup",
        )!.hidden = true;
        control
          .querySelector(".select-trigger")!
          .setAttribute("aria-expanded", "false");
      });
  }
  function calendar(control: HTMLElement, month: string) {
    control.dataset.month = month;
    control.querySelector(".calendar-popup")!.innerHTML = calendarMarkup(
      month,
      control.querySelector<HTMLButtonElement>(".select-trigger")!.value,
    );
  }
  root.addEventListener("click", (event) => {
    const target = event.target as HTMLElement;
    const control = target.closest<HTMLElement>(
      ".select-control, .date-control",
    );
    close(control ?? undefined);
    if (!control) return;
    const trigger =
      control.querySelector<HTMLButtonElement>(".select-trigger")!;
    const popup = control.querySelector<HTMLElement>(
      ".select-options, .calendar-popup",
    )!;
    if (target.closest(".select-trigger")) {
      const opening = popup.hidden;
      if (opening && control.classList.contains("date-control"))
        calendar(control, control.dataset.month!);
      popup.hidden = !opening;
      trigger.setAttribute("aria-expanded", String(opening));
      if (opening)
        popup
          .querySelector<HTMLElement>(
            '[aria-selected="true"], [tabindex="0"], [role="option"]',
          )
          ?.focus();
    }
    const option = target.closest<HTMLButtonElement>(
      "[role=option], [data-date], [data-clear-date]",
    );
    if (option) {
      trigger.value = option.dataset.value ?? option.dataset.date ?? "";
      if (control.classList.contains("select-control")) {
        trigger.querySelector("span")!.textContent =
          option.querySelector("span")!.textContent;
        popup
          .querySelectorAll("[role=option]")
          .forEach((item) =>
            item.setAttribute("aria-selected", String(item === option)),
          );
      } else {
        trigger.querySelector("span")!.textContent = trigger.value
          ? formatDate(trigger.value)
          : uk.allDates;
      }
      close();
      trigger.focus();
      trigger.dispatchEvent(new Event("change", { bubbles: true }));
    }
    const step = target.closest<HTMLElement>("[data-month-step]");
    if (step) {
      const date = new Date(`${control.dataset.month}-01T12:00:00Z`);
      date.setUTCMonth(date.getUTCMonth() + Number(step.dataset.monthStep));
      calendar(control, dateKey(date).slice(0, 7));
      control
        .querySelector<HTMLElement>(
          `[data-month-step="${step.dataset.monthStep}"]`,
        )
        ?.focus();
    }
  });
  root.addEventListener("keydown", (event) => {
    const target = event.target as HTMLElement;
    const control = target.closest<HTMLElement>(
      ".select-control, .date-control",
    );
    if (!control) return;
    const trigger =
      control.querySelector<HTMLButtonElement>(".select-trigger")!;
    if (event.key === "Escape" || event.key === "Tab") {
      close();
      if (event.key === "Escape") {
        event.preventDefault();
        trigger.focus();
        event.stopPropagation();
      }
      return;
    }
    if (
      control.classList.contains("select-control") &&
      ["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)
    ) {
      event.preventDefault();
      const popup = control.querySelector<HTMLElement>(".select-options")!;
      const options = [
        ...popup.querySelectorAll<HTMLButtonElement>("[role=option]"),
      ];
      const current = options.indexOf(target as HTMLButtonElement);
      if (popup.hidden) {
        trigger.click();
        return;
      }
      const index =
        event.key === "Home"
          ? 0
          : event.key === "End"
            ? options.length - 1
            : Math.max(
                0,
                Math.min(
                  options.length - 1,
                  current + (event.key === "ArrowDown" ? 1 : -1),
                ),
              );
      options[index]?.focus();
    }
    if (
      target.dataset.date &&
      [
        "ArrowLeft",
        "ArrowRight",
        "ArrowUp",
        "ArrowDown",
        "Home",
        "End",
      ].includes(event.key)
    ) {
      event.preventDefault();
      const date = new Date(`${target.dataset.date}T12:00:00Z`);
      const weekday = (date.getUTCDay() + 6) % 7;
      const delta = {
        ArrowLeft: -1,
        ArrowRight: 1,
        ArrowUp: -7,
        ArrowDown: 7,
        Home: -weekday,
        End: 6 - weekday,
      }[event.key]!;
      date.setUTCDate(date.getUTCDate() + delta);
      const key = dateKey(date);
      if (!key.startsWith(control.dataset.month!))
        calendar(control, key.slice(0, 7));
      control.querySelectorAll<HTMLElement>("[data-date]").forEach((day) => {
        day.tabIndex = day.dataset.date === key ? 0 : -1;
      });
      control.querySelector<HTMLElement>(`[data-date="${key}"]`)?.focus();
    }
  });
  document.addEventListener("click", (event) => {
    if (!root.contains(event.target as Node)) close();
  });

  let drag: {
    rail: HTMLElement;
    x: number;
    left: number;
    moved: boolean;
  } | null = null;
  let suppressClick = false;
  root.addEventListener("pointerdown", (event) => {
    const rail = (event.target as HTMLElement).closest<HTMLElement>(
      ".archive-dates",
    );
    if (rail && event.pointerType === "mouse" && event.button === 0) {
      drag = { rail, x: event.clientX, left: rail.scrollLeft, moved: false };
      suppressClick = false;
    }
  });
  root.addEventListener("pointermove", (event) => {
    if (!drag) return;
    if (Math.abs(event.clientX - drag.x) > 6) drag.moved = true;
    if (drag.moved) {
      drag.rail.setPointerCapture?.(event.pointerId);
      drag.rail.classList.add("is-dragging");
      drag.rail.scrollLeft = drag.left - (event.clientX - drag.x);
    }
  });
  const endDrag = () => {
    if (drag) {
      suppressClick = drag.moved;
      drag.rail.classList.remove("is-dragging");
      slideCarousel(drag.rail, 0);
      drag = null;
    }
  };
  root.addEventListener("pointerup", endDrag);
  root.addEventListener("pointercancel", endDrag);
  root.addEventListener(
    "click",
    (event) => {
      if (
        suppressClick &&
        (event.target as HTMLElement).closest(".archive-dates")
      ) {
        event.preventDefault();
        event.stopImmediatePropagation();
        suppressClick = false;
      }
      const arrow = (event.target as HTMLElement).closest<HTMLElement>(
        "[data-slide]",
      );
      if (arrow) {
        const rail =
          arrow.parentElement!.querySelector<HTMLElement>(".archive-dates")!;
        slideCarousel(rail, Number(arrow.dataset.slide));
      }
    },
    true,
  );
  root.addEventListener(
    "scroll",
    (event) => {
      if ((event.target as HTMLElement).classList?.contains("archive-dates"))
        syncCarousel(event.target as HTMLElement);
    },
    true,
  );
  window.addEventListener("resize", () => refreshCarousels(root));
}

function syncCarousel(rail: HTMLElement) {
  const buttons =
    rail.parentElement!.querySelectorAll<HTMLButtonElement>("[data-slide]");
  if (buttons.length !== 2) return;
  buttons[0]!.disabled = rail.scrollLeft <= 1;
  buttons[1]!.disabled =
    rail.scrollLeft >=
    Number(rail.dataset.slideMax ?? rail.scrollWidth - rail.clientWidth) - 1;
}
function slideCarousel(rail: HTMLElement, direction: number): void {
  const step = Number(rail.dataset.slideStep) || rail.clientWidth;
  const maximum = Number(
    rail.dataset.slideMax ?? rail.scrollWidth - rail.clientWidth,
  );
  const target = Math.max(
    0,
    Math.min(maximum, (Math.round(rail.scrollLeft / step) + direction) * step),
  );
  rail.scrollBy({
    left: target - rail.scrollLeft,
    behavior: matchMedia("(prefers-reduced-motion: reduce)").matches
      ? "auto"
      : "smooth",
  });
}
export function refreshCarousels(
  root: HTMLElement,
  previousScroll?: number,
): void {
  root.querySelectorAll<HTMLElement>(".archive-dates").forEach((rail) => {
    const items = [...rail.querySelectorAll<HTMLButtonElement>("button")];
    if (!items.length || rail.clientWidth <= 0) return;
    const style = getComputedStyle(rail);
    const padding =
      (parseFloat(style.paddingLeft) || 0) +
      (parseFloat(style.paddingRight) || 0);
    const gap = parseFloat(style.columnGap) || 8;
    const available = rail.clientWidth - padding;
    const minimum = (160 * (parseFloat(style.fontSize) || 16)) / 16;
    const visible = Math.min(
      items.length,
      Math.max(1, Math.floor((available + gap) / (minimum + gap))),
    );
    const width = (available - (visible - 1) * gap) / visible;
    const step = width + gap;
    const oldStep = Number(rail.dataset.slideStep) || step;
    const position = previousScroll ?? rail.scrollLeft;
    rail.style.setProperty("--archive-card-width", `${width}px`);
    rail.dataset.slideStep = String(step);
    const maximum = Math.max(0, (items.length - visible) * step);
    rail.dataset.slideMax = String(maximum);
    rail.scrollLeft = Math.max(
      0,
      Math.min(maximum, Math.round(position / oldStep) * step),
    );
    const chosen = rail.querySelector<HTMLElement>('[aria-pressed="true"]');
    if (
      previousScroll === undefined &&
      chosen &&
      (chosen.offsetLeft < rail.scrollLeft ||
        chosen.offsetLeft + chosen.offsetWidth >
          rail.scrollLeft + rail.clientWidth)
    )
      rail.scrollLeft = Math.min(
        maximum,
        items.indexOf(chosen as HTMLButtonElement) * step,
      );
    syncCarousel(rail);
  });
}
