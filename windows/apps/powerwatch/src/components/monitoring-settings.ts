import { uk } from "../strings";
import { errorCode, setMonitoring } from "../service";
import type { MonitoringSettings, SessionState } from "../types";
import { toggle, frequencyChoice } from "./controls";

const timeValue = (minute: number): string =>
  `${String(Math.floor(minute / 60)).padStart(2, "0")}:${String(minute % 60).padStart(2, "0")}`;
const minutes = (value: string): number => {
  const [hour, minute] = value.split(":").map(Number);
  return (hour ?? NaN) * 60 + (minute ?? NaN);
};

/** Form state survives unrelated dashboard projections; Rust validates and owns
 * the policy. Saving preferences does not fetch the provider. */
export class MonitoringSettingsForm {
  readonly element = document.createElement("form");
  private saving = false;
  constructor(
    container: HTMLElement,
    state: SessionState,
    onSaved: (value: SessionState) => void,
    save: typeof setMonitoring = setMonitoring,
    platform: "windows" | "android" | "other" = "other",
  ) {
    const window = state.monitoring.workWindow;
    this.element.id = "monitoring-settings";
    this.element.className = "monitoring-form settings-section";
    this.element.innerHTML = `<h3>${uk.monitoringTitle}</h3>${toggle(uk.monitoringEnabled, state.monitoring.enabled, 'name="enabled"')}<fieldset><legend>${uk.frequency}</legend>${frequencyChoice(state.checkIntervalMinutes)}</fieldset>
      ${toggle(uk.customHours, !!window, 'name="work-mode" value="window"', uk.customHoursHint)}
      <fieldset class="time-window" id="work-window" ${!window ? "disabled" : ""}><div><label for="work-from">${uk.workFrom}</label><input id="work-from" name="from" type="time" step="60" value="${timeValue(window?.startMinute ?? 420)}" required></div><div><label for="work-until">${uk.workUntil}</label><input id="work-until" name="until" type="time" step="60" value="${timeValue(window?.endMinute ?? 0)}" required></div></fieldset>
      <p class="muted">${uk.workWindowHint}</p><p class="muted">${uk.manualOutsideHours}</p><p class="notice">${uk.monitoringLimits[platform]}</p><div class="save-footer"><button class="primary-button" type="submit">${uk.saveMonitoring}</button><p class="monitoring-result" role="status" hidden></p></div>`;
    this.element.addEventListener("change", () => {
      const enabled =
        this.element.querySelector<HTMLInputElement>(
          '[name="work-mode"]',
        )?.checked;
      this.element.querySelector<HTMLFieldSetElement>(
        "#work-window",
      )!.disabled = !enabled;
      this.element.querySelector<HTMLElement>(".monitoring-result")!.hidden =
        true;
    });
    this.element.addEventListener("submit", (event) => {
      event.preventDefault();
      if (this.saving || !this.element.reportValidity()) return;
      const fields = new FormData(this.element);
      const monitoring: MonitoringSettings = {
        enabled: fields.has("enabled"),
        workWindow:
          fields.get("work-mode") === "window"
            ? {
                startMinute: minutes(String(fields.get("from"))),
                endMinute: minutes(String(fields.get("until"))),
              }
            : null,
      };
      this.saving = true;
      this.element.inert = true;
      const button = this.element.querySelector<HTMLButtonElement>(
        'button[type="submit"]',
      )!;
      const message =
        this.element.querySelector<HTMLElement>(".monitoring-result")!;
      button.textContent = uk.saving;
      void save(Number(fields.get("interval")), monitoring)
        .then((value) => {
          message.setAttribute("role", "status");
          message.textContent = uk.monitoringSaved;
          onSaved(value);
        })
        .catch((error) => {
          const code = errorCode(error);
          message.setAttribute("role", "alert");
          message.textContent =
            code in uk.errors
              ? uk.errors[code as keyof typeof uk.errors]
              : uk.errors.internal_error;
        })
        .finally(() => {
          this.saving = false;
          this.element.inert = false;
          button.textContent = uk.saveMonitoring;
          message.hidden = false;
        });
    });
    container.replaceChildren(this.element);
  }
}
