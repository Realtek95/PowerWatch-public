import { uk } from "../strings";
import { escapeHtml as html, formatMoment } from "../format";
import {
  getBackgroundStatus,
  openBackgroundSettings,
  openExactReminderSettings,
  setNotificationPreferences,
  setPlatformSettings,
  testNotification,
  errorCode,
} from "../service";
import type { BackgroundStatus, NotificationPreferences } from "../types";
import { toggle } from "./controls";

export function journalMarkup(value: BackgroundStatus): string {
  value = {
    ...value,
    notifications: value.notifications.filter(
      (record) => record.state !== "planned" && record.state !== "cancelled",
    ),
  };
  const label = (state: string) =>
    uk.notificationStates[state as keyof typeof uk.notificationStates] ?? state;
  return `<div class="button-row"><button class="text-button" data-action="notifications">${uk.reloadJournal}</button></div>
    ${value.notifications.length ? `<button class="text-button danger-text" data-action="clear-journal">${uk.clearJournal}</button><ol class="notification-journal">${value.notifications.map((record) => `<li><strong>${html(record.plan.title)}</strong><p>${html(record.plan.body)}</p><small>${html(formatMoment(record.updatedAt))}${label(record.state) ? ` · ${html(label(record.state))}` : ""}</small><button class="text-button" data-action="clear-journal" data-journal-id="${record.id}" aria-label="${uk.removeJournalEntry}: ${html(record.plan.title)}">${uk.removeJournalEntry}</button></li>`).join("")}</ol>` : `<p class="muted">${uk.notificationsEmpty}</p>`}`;
}
const time = (minute: number) =>
  `${String(Math.floor(minute / 60)).padStart(2, "0")}:${String(minute % 60).padStart(2, "0")}`;
const minute = (value: FormDataEntryValue | null) => {
  const [h, m] = String(value).split(":").map(Number);
  return (h ?? NaN) * 60 + (m ?? NaN);
};
/** Update permission feedback without replacing unsaved notification choices. */
export function updateReminderStatus(
  container: HTMLElement,
  status: BackgroundStatus["reminderStatus"],
): void {
  const target = container.querySelector<HTMLElement>("[data-reminder-status]");
  if (!target || !status) return;
  target.innerHTML = `<h4>${uk.exactReminders}</h4><p class="muted" role="status">${status.exactAllowed ? uk.exactRemindersAllowed : uk.exactRemindersDenied}</p>${status.exactAccessRequired ? `<button class="secondary-button" data-local="exact-reminders">${status.exactAllowed ? uk.exactRemindersManage : uk.exactRemindersAllow}</button>` : ""}${status.batterySaver || status.backgroundRestricted ? `<p class="muted">${uk.reminderBatteryRestricted}</p>` : ""}`;
}
/** Retained DOM keeps unfinished edits intact while the clock projects the UI. */
export class NotificationSettingsForm {
  readonly element = document.createElement("section");
  constructor(
    container: HTMLElement,
    private readonly mode: "all" | "notifications" | "background" = "all",
  ) {
    this.element.id =
      mode === "background" ? "platform-settings" : "background-settings";
    container.replaceChildren(this.element);
    void this.load();
  }
  private async load() {
    this.element.textContent = uk.loading;
    try {
      const value = await getBackgroundStatus();
      this.render(value);
    } catch (error) {
      this.element.textContent = this.message(error);
    }
  }
  private message(error: unknown) {
    const code = errorCode(error);
    return (
      uk.errors[code as keyof typeof uk.errors] ?? uk.errors.internal_error
    );
  }
  private render(value: BackgroundStatus) {
    let prefs = value.preferences;
    const check = (name: string, label: string, checked: boolean) =>
      toggle(label, checked, `name="${name}"`);
    this.element.innerHTML = `<form class="monitoring-form settings-section"><h3>${uk.notificationSettings}</h3>${value.platform === "android" ? "" : check("enabled", uk.notificationEnabled, prefs.enabled)}<fieldset><legend>${uk.reminderBefore}</legend><p class="muted">${uk.reminderHint}</p><div class="reminder-choices">${[60, 30, 15, 10, 5].map((lead) => check(`lead-${lead}`, uk.reminderLead(lead), prefs.leadMinutes.includes(lead))).join("")}</div></fieldset><fieldset><legend>${uk.notificationEvents}</legend><div class="switch-group">${Object.entries(
      uk.notificationKinds,
    )
      .map(([key, label]) =>
        check(key, label, prefs[key as keyof NotificationPreferences] === true),
      )
      .join(
        "",
      )}</div></fieldset><fieldset aria-label="${uk.quietHours}">${check("quiet", uk.quietHours, prefs.quietHours !== null)}<p class="muted">${uk.quietHint}</p><fieldset class="time-window quiet-window" ${prefs.quietHours ? "" : "disabled"}><div><label for="quiet-from">${uk.workFrom}</label><input id="quiet-from" type="time" name="quiet-from" value="${time(prefs.quietHours?.startMinute ?? 1380)}" required></div><div><label for="quiet-until">${uk.workUntil}</label><input id="quiet-until" type="time" name="quiet-until" value="${time(prefs.quietHours?.endMinute ?? 420)}" required></div></fieldset></fieldset><div class="switch-group">${check("sound", uk.sound, prefs.sound)}${value.platform === "android" ? check("vibration", uk.vibration, prefs.vibration) : ""}</div><div class="save-footer"><button class="primary-button" type="submit">${uk.saveNotifications}</button><p class="result" role="status" hidden></p></div></form>
    ${value.platform === "windows" ? `<form class="platform-form settings-section"><h3>${uk.windowsSettings}</h3><div class="switch-group">${check("tray", uk.tray, value.platformSettings.tray)}${check("autostart", uk.autostart, value.platformSettings.autostart)}</div><div class="save-footer"><button class="primary-button" type="submit">${uk.savePlatform}</button><p class="result" role="status" hidden></p></div></form>` : ""}
    ${value.platform === "android" ? `<section class="settings-section android-background"><h3>${uk.androidBackgroundSettings}</h3><div data-reminder-status></div><p class="muted">${uk.androidBackgroundHelp}</p><button class="secondary-button" data-local="background-settings">${uk.androidBackgroundSettings}</button><p class="result" role="status" hidden></p></section>` : ""}
    <section class="settings-section notification-test"><h3>${uk.testSection}</h3><p class="muted">${uk.notificationLimits[value.platform]}</p><div class="button-row"><button class="secondary-button" data-local="test">${uk.testNotification}</button></div><p class="result" role="status" hidden></p></section>`;
    if (this.mode === "background") {
      this.element.querySelector(".monitoring-form")?.remove();
      this.element.querySelector(".notification-test")?.remove();
      this.element.querySelector("[data-reminder-status]")?.remove();
    } else if (this.mode === "notifications") {
      this.element.querySelector(".platform-form")?.remove();
      const background = this.element.querySelector(".android-background");
      if (background) {
        background.querySelector("h3")?.remove();
        background
          .querySelector('button[data-local="background-settings"]')
          ?.remove();
        background.querySelector("p.muted")?.remove();
      }
    }
    updateReminderStatus(this.element, value.reminderStatus);
    const perform = async (
      section: HTMLElement,
      work: () => Promise<unknown>,
      message: string,
    ) => {
      if (section.inert) return;
      const result = section.querySelector<HTMLElement>(".result")!;
      const submit = section.querySelector<HTMLButtonElement>(
        'button[type="submit"]',
      );
      const label = submit?.textContent;
      section.inert = true;
      if (submit) submit.textContent = uk.saving;
      try {
        await work();
        result.textContent = message;
        result.setAttribute("role", "status");
      } catch (error) {
        result.textContent = this.message(error);
        result.setAttribute("role", "alert");
      } finally {
        section.inert = false;
        if (submit) submit.textContent = label ?? "";
        result.hidden = !result.textContent;
      }
    };
    this.element
      .querySelector(".monitoring-form")
      ?.addEventListener("submit", (event) => {
        event.preventDefault();
        const form = event.target as HTMLFormElement;
        form.querySelector<HTMLFieldSetElement>(".quiet-window")!.disabled =
          !form.querySelector<HTMLInputElement>('[name="quiet"]')!.checked;
        if (!form.reportValidity()) return;
        const fields = new FormData(form);
        const preferences: NotificationPreferences = {
          ...prefs,
          enabled: value.platform === "android" ? true : fields.has("enabled"),
          leadMinutes: [60, 30, 15, 10, 5].filter((lead) =>
            fields.has(`lead-${lead}`),
          ),
          quietHours: fields.has("quiet")
            ? {
                startMinute: minute(fields.get("quiet-from")),
                endMinute: minute(fields.get("quiet-until")),
              }
            : null,
          sound: fields.has("sound"),
          vibration:
            value.platform === "android"
              ? fields.has("vibration")
              : prefs.vibration,
        };
        for (const key of Object.keys(
          uk.notificationKinds,
        ) as (keyof typeof uk.notificationKinds)[])
          preferences[key] = fields.has(key);
        void perform(
          form,
          async () => {
            await setNotificationPreferences(preferences);
            prefs = preferences;
          },
          uk.notificationsSaved,
        );
      });
    this.element
      .querySelector('[name="quiet"]')
      ?.addEventListener("change", () => {
        this.element.querySelector<HTMLFieldSetElement>(
          ".quiet-window",
        )!.disabled =
          !this.element.querySelector<HTMLInputElement>('[name="quiet"]')!
            .checked;
      });
    this.element
      .querySelector(".platform-form")
      ?.addEventListener("submit", (event) => {
        event.preventDefault();
        const fields = new FormData(event.target as HTMLFormElement);
        void perform(
          event.target as HTMLFormElement,
          () =>
            setPlatformSettings({
              tray: fields.has("tray"),
              autostart: fields.has("autostart"),
            }),
          uk.platformSaved,
        );
      });
    this.element.addEventListener("click", (event) => {
      const action = (event.target as HTMLElement).closest<HTMLButtonElement>(
        "[data-local]",
      )?.dataset.local;
      const section = (event.target as HTMLElement).closest<HTMLElement>(
        ".settings-section",
      )!;
      if (action === "test")
        void perform(section, testNotification, uk.notificationQueued);
      if (action === "exact-reminders")
        void perform(
          section,
          openExactReminderSettings,
          uk.systemSettingsOpened,
        );
      if (action === "background-settings")
        void perform(section, openBackgroundSettings, uk.systemSettingsOpened);
    });
  }
}
