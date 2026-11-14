import "./compat";
import "./style.css";
import { uk } from "./strings";
import { icon, type IconName } from "./icons";
import {
  escapeHtml as html,
  eventCopy,
  formatDate,
  formatDuration,
  formatRange,
  formatTime,
  formatMoment,
} from "./format";
import {
  activateAddress,
  completeOnboarding,
  deleteAddress,
  errorCode as commandErrorCode,
  getDashboard,
  getSession,
  getRuntimeInfo,
  getAppInfo,
  setAutomaticUpdates,
  openAppLink,
  checkUpdates,
  installUpdate,
  projectDashboard,
  getArchive,
  getViewPreferences,
  setViewPreferences,
  getBackgroundStatus,
  clearNotificationJournal,
  setAddressOptions,
} from "./service";
import {
  NotificationSettingsForm,
  journalMarkup,
  updateReminderStatus,
} from "./components/notification-settings";
import { listen } from "@tauri-apps/api/event";
import { isTauri } from "@tauri-apps/api/core";
import { AddressWizard } from "./components/address-wizard";
import { aboutMarkup, sourcesMarkup, privacyMarkup } from "./components/about";
import { evidenceMarkup } from "./components/evidence";
import { updateMarkup } from "./components/updates";
import {
  diagnosticsMarkup,
  sourceReportMarkup,
} from "./components/diagnostics";
import { retainFocus } from "./components/focus";
import { MonitoringSettingsForm } from "./components/monitoring-settings";
import {
  toggle,
  frequencyChoice,
  installControls,
  refreshCarousels,
} from "./components/controls";
import {
  archiveMarkup,
  statisticsMarkup,
  changesMarkup,
  changeDetails,
} from "./components/history";
import type {
  AppInfo,
  Dashboard,
  RuntimeInfo,
  SessionState,
  ArchiveView,
} from "./types";

type Page =
  | "home"
  | "chart"
  | "history"
  | "statistics"
  | "settings"
  | "addresses"
  | "diagnostics"
  | "changes";
const root = document.querySelector<HTMLDivElement>("#app");
if (!root) throw new Error("Missing app root");
const app: HTMLDivElement = root;
installControls(app);
app.innerHTML = `<div class="app-shell"></div><dialog id="details"><div class="dialog-header"><h2 id="dialog-title"></h2><button class="icon-button" data-action="close-dialog" aria-label="${uk.close}">${icon("x")}</button></div><div id="dialog-content"></div></dialog>`;
const shell = app.querySelector<HTMLDivElement>(".app-shell")!;
let dialogFocus: () => void = () => {};
app.querySelector("#details")?.addEventListener("close", () => dialogFocus());
let appInfo: AppInfo | null = null;
let aboutError = false;
let notificationsAllowed: boolean | null = null;
let permissionRevision = 0;
let onboardingInterval: number | null = null;
let screenKey = "";
let pageTransitionAt = -Infinity;
let aboutLoading = false;
let data: Dashboard | null = null;
let session: SessionState | null = null;
let addressId: string | null = null;
let dayOffset = 0;
let page: Page = "home";
let mode: "scale" | "list" =
  localStorage.getItem("schedule-view") === "list" ? "list" : "scale";
let receivedAt = performance.now();
let errorCode: keyof typeof uk.errors | null = null;
let loading = false;
let refreshQueued = false;
let requestVersion = 0;
let runtime: RuntimeInfo = {
  developerTools: false,
  localProjection: false,
  now: new Date().toISOString(),
};
let clockAnchor = Date.now();
let lastAttempt: string | null = null;
let projecting = false;
let projectionExpired = false;
let archiveView: ArchiveView | null = null;
let archiveDate: string | null = null;
let historyVersion = 0;
let changeDate = "";

function currentNow(): number {
  return data
    ? Date.parse(data.now) + performance.now() - receivedAt
    : clockAnchor + performance.now() - receivedAt;
}
function segmentLabel(value: Dashboard, index: number): string {
  const interval = value.selectedDay.intervals[index];
  return interval
    ? `${formatRange(interval)} · ${uk.states[interval.state]} · ${formatDuration((Date.parse(interval.end) - Date.parse(interval.start)) / 1000)}`
    : "";
}
function errorMessage(): string {
  return errorCode ? uk.errors[errorCode] : "";
}

function scheduleMarkup(value: Dashboard, archiveMode = false): string {
  const bands = value.bands
    .map(
      (
        band,
      ) => `<div class="half-day"><div class="half-label">${band.label}</div><div class="band-body">
    <div class="ticks">${band.ticks.map((tick) => `<span style="top:${tick.position * 100}%">${html(tick.label)}</span>`).join("")}</div>
    <div class="track">${band.segments.map((segment) => `<button class="segment ${segment.state}" style="top:${segment.top * 100}%;height:${segment.height * 100}%" data-interval="${segment.intervalIndex}" aria-label="${html(segmentLabel(value, segment.intervalIndex))}"><span class="segment-pattern"></span></button>`).join("")}
    <div class="now-marker" data-band-start="${band.start}" data-band-end="${band.end}" hidden><span></span><b></b></div></div>
    </div></div>`,
    )
    .join("");
  return `<section class="card schedule-card" aria-labelledby="schedule-title">
    <div class="section-heading"><h2 id="schedule-title">${icon("calendar")}${uk.schedule}</h2>${archiveMode ? "" : `<div class="day-tabs" role="group" aria-label="${uk.schedule}">${[uk.today, uk.tomorrow].map((name, i) => `<button data-day="${i}" aria-pressed="${dayOffset === i}" class="${dayOffset === i ? "selected" : ""}">${name}</button>`).join("")}</div>`}</div>
    <div class="schedule-toolbar"><span>${html(formatDate(value.selectedDay.date))}</span><div class="view-tabs"><button data-mode="scale" aria-pressed="${mode === "scale"}">${uk.scale}</button><button data-mode="list" aria-pressed="${mode === "list"}">${uk.list}</button></div></div>
    ${value.clockChange ? `<p class="notice">${uk.clockChange}</p>` : ""}
    ${
      mode === "scale"
        ? `<div class="day-bands">${bands}</div><div class="legend">${Object.entries(
            uk.states,
          )
            .map(
              ([state, label]) =>
                `<span><i class="swatch ${state}"></i>${label}</span>`,
            )
            .join("")}</div>`
        : `<div class="interval-list">${value.selectedDay.intervals.map((item, i) => `<button data-interval="${i}"><i class="swatch ${item.state}"></i><span><b>${formatRange(item)}</b><small>${uk.states[item.state]}</small></span><span>${formatDuration((Date.parse(item.end) - Date.parse(item.start)) / 1000)}</span></button>`).join("")}</div>`
    }
    <div class="day-summary">${uk.plannedOutage}: <strong>${formatDuration(value.knownOutageSeconds)}</strong>${value.hasUncertainty ? `<span> · ${uk.knownOnly}</span>` : ""}</div>
  </section>`;
}

function emptyCard(kind: "history" | "statistics", compact = false): string {
  const isHistory = kind === "history";
  if (compact) {
    const stats = archiveView?.statistics;
    const text = stats
      ? isHistory
        ? `${uk.fullDays}: ${stats.completeDays} · ${uk.partialDays}: ${stats.partialDays} · ${uk.missingDays}: ${stats.missingDays}`
        : `${uk.plannedOutage}: ${formatDuration(stats.knownOutageSeconds)} · ${uk.knownOnly}`
      : uk.loading;
    return `<section class="card empty-card"><div class="section-heading"><h2>${icon(isHistory ? "history" : "chart")}${isHistory ? uk.history : uk.statistics}</h2><button class="plain-button" data-page="${kind}" aria-label="${isHistory ? uk.history : uk.statistics}">${icon("chevron")}</button></div><p class="section-caption">${html(text)}</p></section>`;
  }
  return `<section class="card empty-card"><div class="section-heading"><h2>${icon(isHistory ? "history" : "chart")}${isHistory ? uk.history : uk.statistics}</h2>${compact ? `<button class="plain-button" data-page="${kind}" aria-label="${isHistory ? uk.history : uk.statistics}">${icon("chevron")}</button>` : ""}</div>
    ${isHistory ? `<p class="section-caption">${uk.lastSevenDays}</p>` : ""}
    <div class="empty-content"><span class="empty-icon">${icon(isHistory ? "history" : "chart")}</span><div><h3>${isHistory ? uk.historyEmpty : uk.statisticsEmpty}</h3><p>${isHistory ? uk.historyBody : uk.statisticsBody}</p></div></div></section>`;
}

function settingsMarkup(): string {
  const theme = localStorage.getItem("theme") ?? "system";
  const large = localStorage.getItem("font-size") === "large";
  return `<section class="card settings-card"><h2>${icon("settings")}${uk.settings}</h2><button class="settings-link" data-page="addresses">${icon("pin")}${uk.addresses}${icon("chevron")}</button><h3>${uk.appearance}</h3><div class="theme-buttons">${(["system", "light", "dark"] as const).map((key) => `<button data-theme="${key}" aria-pressed="${theme === key}">${uk[key]}</button>`).join("")}</div>
    <h3>${uk.fontSize}</h3><div class="theme-buttons"><button data-font="normal" aria-pressed="${!large}">${uk.normalText}</button><button data-font="large" aria-pressed="${large}">${uk.largeText}</button></div><p class="muted settings-inline-note">${uk.appearanceSaved}</p><div id="monitoring-preferences"></div><div id="background-preferences"></div>
    <button class="settings-link" data-action="notifications">${icon("bell")}${uk.notificationJournal}${icon("chevron")}</button><button class="settings-link" data-page="changes">${icon("history")}${uk.changesTitle}${icon("chevron")}</button><button class="settings-link" data-page="diagnostics">${icon("info")}${uk.diagnosticsTitle}${icon("chevron")}</button>
    ${aboutMarkup(appInfo, aboutError)}
  </section>`;
}

function onboardingMarkup(): string {
  if (!session?.addresses.length)
    return `<section class="card welcome-card"><div class="welcome-mark brand-icon">${icon("zap")}</div><h2>${uk.welcomeTitle}</h2><p>${uk.welcomeBody}</p><div class="settings-note">${icon("info")}<p>${uk.liveHint}</p></div><button class="primary-button" data-action="add-address">${uk.start}</button><p class="session-note">${uk.addressSessionNote}</p></section>`;
  const selected = session.addresses.find(
    (entry) => entry.id === session?.activeAddressId,
  );
  return `<section class="card welcome-card"><h2>${uk.preferencesTitle}</h2><p>${uk.selectedAddress}: <strong>${html(selected?.address.label ?? "")}</strong></p><form id="onboarding-preferences"><fieldset><legend>${uk.frequency}</legend>${frequencyChoice(onboardingInterval ?? session.checkIntervalMinutes)}</fieldset><div class="settings-note">${icon("bell")}<div><h3>${uk.notificationsPending}</h3><p>${uk.notificationsExplanation}</p></div></div><button class="primary-button" type="submit">${uk.openSchedule}</button></form><button class="text-button" data-action="edit-address" data-id="${html(selected?.id ?? "")}">${uk.editAddress}</button></section>`;
}

function addressesMarkup(): string {
  return `<section class="card addresses-page"><div class="section-heading"><h2>${icon("pin")}${uk.addresses}</h2><button class="text-button" data-page="home">${uk.back}</button></div><p class="session-note">${uk.addressSessionNote}</p><div class="saved-addresses">${session?.addresses.map((entry) => `<article class="saved-address ${entry.id === addressId ? "is-active" : ""}"><button class="address-choice" data-action="activate-address" data-id="${html(entry.id)}" aria-label="${uk.makeActive}: ${html(entry.address.label)}"><span><strong>${html(entry.address.label)}</strong><span>${html(entry.address.settlement)}, ${html(entry.address.street)}, ${html(entry.address.house)}</span><small>${entry.address.queue ? `${uk.group} ${html(entry.address.queue)}` : uk.queueUnknown}</small></span>${entry.id === addressId ? icon("check") : icon("chevron")}</button>${toggle(uk.monitorAddress, entry.monitoringEnabled, `data-address-monitor="${html(entry.id)}"`)}${toggle(uk.notifyAddress, entry.notificationsEnabled, `data-address-notify="${html(entry.id)}"`)}<div class="address-row-actions"><span>${entry.id === addressId ? uk.activeAddress : ""}</span><button class="text-button" data-action="edit-address" data-id="${html(entry.id)}">${uk.editAddress}</button><button class="text-button danger-text" data-action="delete-address" data-id="${html(entry.id)}">${uk.deleteAddress}</button></div></article>`).join("") ?? ""}</div><button class="primary-button" data-action="add-address">${uk.addAddress}</button></section>`;
}

function applySession(value: SessionState): void {
  ++requestVersion;
  clockAnchor = currentNow();
  receivedAt = performance.now();
  if (
    value.onboardingComplete ||
    session?.activeAddressId !== value.activeAddressId
  )
    onboardingInterval = null;
  session = value;
  addressId = value.activeAddressId;
  data = null;
  loading = false;
  refreshQueued = false;
  lastAttempt = null;
  projectionExpired = false;
  archiveView = null;
  ++historyVersion;
  archiveDate = null;
  errorCode = null;
  document.querySelector<HTMLDialogElement>("#details")?.close();
  render();
  if (value.onboardingComplete && addressId) {
    void loadArchive();
    const version = requestVersion;
    if (runtime.localProjection)
      void projectDashboard(addressId, dayOffset)
        .then((result) => {
          if (version !== requestVersion) return;
          data = result;
          refreshQueued = !!result.refreshPending;
          errorCode = (result.lastCheckError as keyof typeof uk.errors) ?? null;
          receivedAt = performance.now();
          render();
        })
        .catch((error: unknown) => {
          if (commandErrorCode(error) === "refresh_queued") {
            refreshQueued = true;
            render();
          }
        });
    else void load();
  }
}

function openAddressWizard(id?: string): void {
  const initial = session?.addresses.find((entry) => entry.id === id) ?? null;
  showDialog(
    initial ? uk.editAddress : uk.addAddress,
    '<div id="address-wizard"></div>',
  );
  const container = document.querySelector<HTMLElement>("#address-wizard");
  if (container) new AddressWizard(container, initial, applySession);
}

async function mutateSession(
  action: () => Promise<SessionState>,
): Promise<void> {
  try {
    applySession(await action());
  } catch (error) {
    const code = commandErrorCode(error);
    showDialog(
      uk.settings,
      `<p role="alert">${html(code in uk.errors ? uk.errors[code as keyof typeof uk.errors] : uk.errors.internal_error)}</p>`,
    );
  }
}

function render(): void {
  const archiveScroll =
    shell.querySelector<HTMLElement>(".archive-dates")?.scrollLeft;
  const restoreFocus = retainFocus(shell);
  const backgroundPreferences =
    page === "settings" ? shell.querySelector("#background-settings") : null;
  const preferences =
    page === "settings" ? shell.querySelector("#monitoring-settings") : null;
  const nextScreen =
    session && !session.onboardingComplete
      ? `onboarding:${session.addresses.length ? "preferences" : "welcome"}`
      : page;
  if (screenKey && nextScreen !== screenKey)
    pageTransitionAt = performance.now();
  screenKey = nextScreen;
  const transitionElapsed = performance.now() - pageTransitionAt;
  const motion =
    transitionElapsed < 320
      ? ` class="page-enter" style="animation-delay:-${Math.max(0, transitionElapsed)}ms"`
      : "";
  const value = data;
  const refreshBusy = loading || refreshQueued;
  const address =
    value?.snapshot.address ??
    session?.addresses.find((entry) => entry.id === addressId)?.address;
  const degraded = projectionExpired || value?.freshness.status !== "current";
  const incomplete =
    value?.selectedDay.intervals.some((part) => part.state === "unknown") ??
    false;
  const event = value ? eventCopy(value, currentNow(), degraded) : null;
  shell.innerHTML = `<header class="app-header"><div class="brand"><div class="brand-icon">${icon("zap")}</div><div><h1>PowerWatch</h1><p>${uk.tagline}</p></div></div><div class="header-actions"><button class="icon-button" data-action="notifications" aria-label="${uk.notifications}">${icon("bell")}</button><button class="icon-button ${refreshBusy ? "is-loading" : ""}" data-action="refresh" aria-label="${uk.refresh}" ${refreshBusy || !session?.onboardingComplete ? "disabled" : ""}>${icon("refresh")}</button></div></header>
    <main${motion}><div id="notification-permission-notice">${permissionMarkup()}</div><div class="context-strip">${icon("info")}<span>${uk.liveHint}</span><span class="current-time"><b data-clock>${value ? formatTime(currentNow()) : "—"}</b></span></div>
    ${projectionExpired ? `<p class="notice" role="status">${uk.projectionExpired}</p>` : ""}
    ${value && value.freshness.status !== "current" ? `<p class="notice" role="status"><strong>${value.freshness.status === "clock_mismatch" ? uk.clockMismatch : uk.staleData}.</strong> ${uk.staleHint}</p>` : ""}
    ${address && session?.onboardingComplete ? `<button class="address-card" data-page="addresses" aria-label="${uk.chooseAddress}">${icon("pin", "address-pin")}<span class="address-text"><small>${html(address.label)}</small><strong>${html(address.settlement)}</strong><span>${html(address.street)}, ${html(address.house)}</span></span><span class="queue-block">${icon("layers")}<span><small>${uk.queue}</small><b>${html(address.queue ?? "—")}</b></span></span>${icon("chevron", "address-chevron")}</button>` : ""}
    ${
      session && !session.onboardingComplete
        ? onboardingMarkup()
        : page === "addresses"
          ? addressesMarkup()
          : page === "history"
            ? archiveMarkup(archiveView, archiveDate) +
              (archiveView?.dashboard
                ? scheduleMarkup(archiveView.dashboard, true)
                : "")
            : page === "statistics"
              ? statisticsMarkup(archiveView)
              : page === "changes"
                ? changesMarkup(archiveView, changeDate)
                : page === "diagnostics"
                  ? diagnosticsMarkup({
                      data,
                      lastAttempt,
                      loading: refreshBusy || projecting,
                    })
                  : page === "settings"
                    ? settingsMarkup()
                    : !value
                      ? `<section class="card loading-card" role="status">${refreshQueued ? uk.refreshQueued : errorCode ? html(errorMessage()) : uk.loading}</section>`
                      : `
      ${
        page === "home"
          ? `<section class="status-card ${degraded ? "unknown" : value.currentState}"><span class="status-icon">${icon(value.currentState === "unknown" || degraded ? "info" : "zap")}</span><div><h2>${uk.statuses[value.currentState]}</h2><p>${degraded ? uk.cachedForecast : uk.forecastNote}</p></div></section>${sourceReportMarkup(value)}
      <section class="card next-event"><div class="event-icon">${icon("calendar")}</div><div class="event-content"><p>${uk.nextEvent}</p><h2 data-event-title>${html(event?.title ?? "")}</h2><span data-event-detail>${icon("clock")}${html(event?.detail ?? "")}</span></div><div class="countdown" data-countdown ${event?.countdown ? "" : "hidden"}>${event?.countdown ?? ""}</div></section>
      <div class="freshness"><span title="${html(formatMoment(value.snapshot.receivedAt))}">${icon("refresh")}${uk.updated}: <b>${formatTime(value.snapshot.receivedAt)}</b></span><span class="freshness-badge ${degraded || incomplete || !!errorCode ? "failed" : ""}">${icon(degraded || incomplete || !!errorCode ? "info" : "check")}${refreshQueued ? uk.refreshQueued : errorCode ? (value.retryPending ? uk.retryPending : uk.checkFailed) : value.freshness.status === "clock_mismatch" ? uk.clockMismatch : degraded ? uk.staleData : incomplete ? uk.partial : uk.currentData}</span></div><p class="freshness-detail">${uk.dataAge}: <span data-age>${formatDuration(value.freshness.ageSeconds)}</span>${value.snapshot.publishedAt ? ` · ${uk.published}: ${html(formatMoment(value.snapshot.publishedAt))}` : ""}</p>`
          : ""
      }
      ${scheduleMarkup(value)}${page === "home" ? evidenceMarkup(value) + emptyCard("history", true) + emptyCard("statistics", true) : ""}`
    }
    <p class="footer-note">${uk.actualPowerNote}</p></main>
    ${
      session?.onboardingComplete
        ? `<nav class="bottom-nav" aria-label="PowerWatch">${(
            [
              ["home", "home", uk.home],
              ["chart", "calendar", uk.chart],
              ["history", "history", uk.historyShort],
              ["statistics", "chart", uk.statisticsShort],
              ["settings", "settings", uk.settings],
            ] as [Page, IconName, string][]
          )
            .map(
              ([key, symbol, label]) =>
                `<button data-page="${key}" class="${page === key ? "active" : ""}" ${page === key ? 'aria-current="page"' : ""}>${icon(symbol)}<span>${label}</span></button>`,
            )
            .join("")}</nav>`
        : ""
    }
    `;
  const preferencesContainer = shell.querySelector<HTMLElement>(
    "#monitoring-preferences",
  );
  if (preferencesContainer && session) {
    if (preferences) preferencesContainer.replaceChildren(preferences);
    else
      new MonitoringSettingsForm(
        preferencesContainer,
        session,
        (value) => {
          session = value;
          void updateProjection();
        },
        undefined,
        runtime.platform ?? "other",
      );
  }
  const backgroundContainer = shell.querySelector<HTMLElement>(
    "#background-preferences",
  );
  if (backgroundContainer) {
    if (backgroundPreferences)
      backgroundContainer.replaceChildren(backgroundPreferences);
    else new NotificationSettingsForm(backgroundContainer);
  }
  restoreFocus();
  refreshCarousels(shell, archiveScroll);
  tick();
}

async function loadAbout(): Promise<void> {
  if (aboutLoading) return;
  aboutLoading = true;
  aboutError = false;
  try {
    appInfo = await getAppInfo();
  } catch {
    aboutError = true;
  } finally {
    aboutLoading = false;
    if (page === "settings") render();
  }
}
let checkingUpdates = false;
let installingUpdate = false;
async function checkForUpdates(automatic: boolean): Promise<void> {
  if (checkingUpdates || installingUpdate) return;
  checkingUpdates = true;
  if (!automatic) showDialog(uk.updateTitle, `<p>${uk.updateChecking}</p>`);
  const version = dialogVersion;
  try {
    const result = await checkUpdates(automatic);
    if (!automatic && version === dialogVersion)
      document.querySelector("#dialog-content")!.innerHTML = updateMarkup(
        result,
        runtime?.platform,
      );
    else if (
      automatic &&
      result.status === "available" &&
      !document.querySelector("#details[open]")
    )
      showDialog(uk.updateTitle, updateMarkup(result, runtime?.platform));
  } catch (error) {
    if (!automatic && version === dialogVersion) {
      const code = commandErrorCode(error) as keyof typeof uk.errors;
      document.querySelector("#dialog-content")!.innerHTML =
        `<p>${uk.errors[code] ?? uk.errors.internal_error}</p>`;
    }
  } finally {
    checkingUpdates = false;
    if (!automatic) void loadAbout();
  }
}
async function runUpdateInstallation(): Promise<void> {
  if (installingUpdate) return;
  installingUpdate = true;
  showDialog(
    uk.updateTitle,
    `<p>${uk.updateDownload}</p><progress id="update-progress" max="100" value="0" aria-label="${uk.updateDownload}"></progress>`,
  );
  const version = dialogVersion;
  try {
    const status = await installUpdate();
    if (version === dialogVersion)
      document.querySelector("#dialog-content")!.innerHTML =
        status === "permission_required"
          ? `<p>${uk.updatePermission}</p><div class="form-actions"><button class="primary-button" data-action="install-update">${uk.updateContinue}</button></div>`
          : `<p>${uk.updateInstallerOpened}</p>`;
  } catch (error) {
    if (version === dialogVersion) {
      const code = commandErrorCode(error) as keyof typeof uk.errors;
      document.querySelector("#dialog-content")!.innerHTML =
        `<p>${uk.errors[code] ?? uk.errors.internal_error}</p>`;
    }
  } finally {
    installingUpdate = false;
  }
}
async function load(): Promise<void> {
  if (!addressId || !session?.onboardingComplete || loading || refreshQueued)
    return;
  const restoreControl = retainFocus(shell);
  const originPage = page;
  const version = ++requestVersion;
  loading = true;
  lastAttempt = new Date(currentNow()).toISOString();
  render();
  try {
    const result = await getDashboard(addressId, dayOffset);
    if (version !== requestVersion) return;
    data = result;
    refreshQueued = !!result.refreshPending;
    receivedAt = performance.now();
    errorCode = (result.lastCheckError as keyof typeof uk.errors) ?? null;
    projectionExpired = false;
    // The provider's receipt time is different from the time of this attempt.
    lastAttempt = result.now;
  } catch (error: unknown) {
    if (version !== requestVersion) return;
    const code =
      typeof error === "object" && error !== null && "code" in error
        ? String(error.code)
        : "internal_error";
    if (code === "refresh_queued") {
      refreshQueued = true;
      errorCode = null;
      projectionExpired = false;
    } else
      errorCode =
        code in uk.errors ? (code as keyof typeof uk.errors) : "internal_error";
    // Keep the controls aligned with the retained trusted snapshot on failure.
    if (data) {
      dayOffset = data.selectedDay.date === data.snapshot.days[0]?.date ? 0 : 1;
    }
  } finally {
    if (version === requestVersion) {
      loading = false;
      render();
      if (
        page === originPage &&
        document.activeElement === document.body &&
        !document.querySelector("#details[open]")
      )
        restoreControl();
    }
  }
}

let dialogVersion = 0;
function showDialog(title: string, content: string): void {
  ++dialogVersion;
  const dialog = document.querySelector<HTMLDialogElement>("#details");
  const heading = document.querySelector("#dialog-title");
  const body = document.querySelector("#dialog-content");
  if (!dialog || !heading || !body) return;
  if (!dialog.open) dialogFocus = retainFocus(shell);
  heading.textContent = title;
  body.innerHTML = content;
  dialog.setAttribute("aria-labelledby", "dialog-title");
  dialog.showModal();
}

async function loadJournal(version: number): Promise<void> {
  try {
    const value = await getBackgroundStatus();
    const dialog = document.querySelector<HTMLDialogElement>("#details");
    if (version === dialogVersion && dialog?.open)
      dialog.querySelector("#dialog-content")!.innerHTML = journalMarkup(value);
  } catch (error) {
    if (version !== dialogVersion) return;
    const code = commandErrorCode(error);
    const body = document.querySelector("#dialog-content");
    if (body)
      body.textContent =
        uk.errors[code as keyof typeof uk.errors] ?? uk.errors.internal_error;
  }
}

async function loadArchive(
  key: string | null = null,
  date: string | null = archiveDate,
): Promise<void> {
  const version = ++historyVersion;
  try {
    const view = await getArchive(key, date);
    if (version !== historyVersion) return;
    archiveView = view;
    archiveDate = date ?? view.days[0]?.date ?? null;
    render();
  } catch (error) {
    if (version === historyVersion)
      showDialog(
        uk.history,
        `<p role="alert">${html(uk.errors[commandErrorCode(error) as keyof typeof uk.errors] ?? uk.errors.internal_error)}</p>`,
      );
  }
}

function tick(): void {
  if (!data) return;
  const now = currentNow();
  const event = eventCopy(data, now, projectionExpired);
  const title = document.querySelector("[data-event-title]");
  if (title) title.textContent = event.title;
  const detail = document.querySelector("[data-event-detail]");
  if (detail) detail.textContent = event.detail;
  const countdown = document.querySelector<HTMLElement>("[data-countdown]");
  if (countdown) {
    countdown.hidden = !event.countdown;
    if (event.countdown) countdown.textContent = event.countdown;
  }
  const age = document.querySelector("[data-age]");
  if (age)
    age.textContent = formatDuration(
      data.freshness.ageSeconds + (now - Date.parse(data.now)) / 1000,
    );
  const clock = document.querySelector("[data-clock]");
  if (clock) clock.textContent = formatTime(now);
  document.querySelectorAll<HTMLElement>(".now-marker").forEach((marker) => {
    const start = Date.parse(marker.dataset.bandStart ?? "");
    const end = Date.parse(marker.dataset.bandEnd ?? "");
    marker.hidden = now < start || now >= end;
    marker.style.top = `${((now - start) / (end - start)) * 100}%`;
    const label = marker.querySelector("b");
    if (label) label.textContent = formatTime(now);
  });
  if (
    !projectionExpired &&
    (Math.floor(now / 60000) !== Math.floor(Date.parse(data.now) / 60000) ||
      (data.nextEvent && now >= Date.parse(data.nextEvent.at)))
  )
    void updateProjection();
}

async function updateProjection(): Promise<void> {
  if (
    !session?.onboardingComplete ||
    !addressId ||
    loading ||
    projecting ||
    !runtime.localProjection
  )
    return;
  projecting = true;
  const version = requestVersion;
  try {
    const result = await projectDashboard(addressId, dayOffset);
    if (version !== requestVersion) return;
    data = result;
    refreshQueued = !!result.refreshPending;
    projectionExpired = false;
    receivedAt = performance.now();
    errorCode = (result.lastCheckError as keyof typeof uk.errors) ?? null;
  } catch (error) {
    if (version === requestVersion) {
      if (commandErrorCode(error) === "refresh_queued") {
        refreshQueued = true;
        errorCode = null;
        projectionExpired = false;
      } else {
        refreshQueued = false;
        projectionExpired = true;
      }
    }
  } finally {
    projecting = false;
    if (version === requestVersion) render();
  }
}

app.addEventListener("click", (event) => {
  const button = (event.target as HTMLElement).closest<HTMLButtonElement>(
    "button",
  );
  if (!button) return;
  if (button.dataset.page) {
    page = button.dataset.page as Page;
    render();
    if (page === "settings") void loadAbout();
    window.scrollTo({ top: 0 });
    if (["history", "statistics", "changes"].includes(page)) void loadArchive();
  }
  if (button.dataset.archiveDate)
    void loadArchive(
      archiveView?.selectedKey ?? null,
      button.dataset.archiveDate,
    );
  if (button.dataset.change) {
    const record = archiveView?.changes.find(
      (item) => item.id === Number(button.dataset.change),
    );
    if (record) showDialog(uk.changesTitle, changeDetails(record));
  }
  if (button.dataset.day) {
    dayOffset = Number(button.dataset.day);
    ++requestVersion;
    projectionExpired = false;
    void updateProjection();
  }
  if (button.dataset.mode) {
    mode = button.dataset.mode as "scale" | "list";
    localStorage.setItem("schedule-view", mode);
    persistViewPreferences();
    render();
  }
  if (button.dataset.theme) {
    localStorage.setItem("theme", button.dataset.theme);
    applyTheme();
    persistViewPreferences();
    render();
  }
  if (button.dataset.font) {
    localStorage.setItem("font-size", button.dataset.font);
    applyTheme();
    persistViewPreferences();
    render();
  }
  if (button.dataset.interval) {
    const interval = (page === "history" ? archiveView?.dashboard : data)
      ?.selectedDay.intervals[Number(button.dataset.interval)];
    if (interval)
      showDialog(
        uk.intervalDetails,
        `<p class="detail-range">${formatRange(interval)}</p><p><i class="swatch ${interval.state}"></i>${uk.states[interval.state]}</p><p>${formatDuration((Date.parse(interval.end) - Date.parse(interval.start)) / 1000)}</p>`,
      );
  }
  if (button.dataset.link)
    void openAppLink(button.dataset.link).catch(() =>
      showDialog(uk.about, `<p>${uk.errors.link_unavailable}</p>`),
    );
  switch (button.dataset.action) {
    case "check-updates":
      void checkForUpdates(false);
      break;
    case "install-update":
      void runUpdateInstallation();
      break;
    case "reload-about":
      void loadAbout();
      break;
    case "about-sources": {
      showDialog(uk.dataSources, `<p>${uk.loadingAppInfo}</p>`);
      const version = dialogVersion;
      void getAppInfo()
        .then((info) => {
          appInfo = info;
          if (version === dialogVersion)
            document.querySelector("#dialog-content")!.innerHTML =
              sourcesMarkup(info);
        })
        .catch(() => {
          if (version === dialogVersion)
            document.querySelector("#dialog-content")!.innerHTML =
              `<p>${uk.errors.internal_error}</p>`;
        });
      break;
    }
    case "about-privacy":
      showDialog(uk.privacy, privacyMarkup());
      break;
    case "all-change-dates":
      changeDate = "";
      render();
      break;
    case "refresh":
      void load();
      break;
    case "notifications":
      showDialog(uk.notificationJournal, `<p>${uk.loading}</p>`);
      void loadJournal(dialogVersion);
      break;
    case "clear-journal": {
      button.disabled = true;
      const version = dialogVersion;
      void clearNotificationJournal(
        button.dataset.journalId ? Number(button.dataset.journalId) : null,
      )
        .then(() => loadJournal(version))
        .catch(() => {
          if (version === dialogVersion)
            showDialog(
              uk.notificationJournal,
              `<p>${uk.errors.storage_error}</p>`,
            );
        });
      break;
    }
    case "add-address":
      openAddressWizard();
      break;
    case "edit-address":
      openAddressWizard(button.dataset.id);
      break;
    case "activate-address":
      if (button.dataset.id) {
        const id = button.dataset.id;
        page = "home";
        void mutateSession(() => activateAddress(id));
      }
      break;
    case "delete-address": {
      const entry = session?.addresses.find(
        (entry) => entry.id === button.dataset.id,
      );
      if (entry)
        showDialog(
          uk.deleteQuestion,
          `<p><strong>${html(entry.address.label)}</strong></p><p>${uk.deleteBody}</p><div class="dialog-actions"><button class="text-button" data-action="close-dialog">${uk.cancel}</button><button class="primary-button danger-button" data-action="confirm-delete" data-id="${html(entry.id)}">${uk.deleteAddress}</button></div>`,
        );
      break;
    }
    case "confirm-delete":
      if (button.dataset.id) {
        const id = button.dataset.id;
        void mutateSession(() => deleteAddress(id));
      }
      break;
    case "close-dialog":
      document.querySelector<HTMLDialogElement>("#details")?.close();
      break;
  }
});
app.addEventListener("submit", (event) => {
  if ((event.target as HTMLFormElement).id !== "onboarding-preferences") return;
  event.preventDefault();
  const form = event.target as HTMLFormElement;
  const interval = Number(new FormData(form).get("interval"));
  form.inert = true;
  void mutateSession(() => completeOnboarding(interval)).finally(() => {
    form.inert = false;
  });
});
app.addEventListener("change", (event) => {
  const select = event.target as HTMLSelectElement;
  const input = event.target as HTMLInputElement;
  if (input.name === "interval" && input.closest("#onboarding-preferences"))
    onboardingInterval = Number(input.value);
  if (input.id === "automatic-updates") {
    input.disabled = true;
    void setAutomaticUpdates(input.checked)
      .then(() => loadAbout())
      .catch(() => {
        input.disabled = false;
        input.checked = !input.checked;
        showDialog(uk.about, `<p>${uk.errors.storage_error}</p>`);
      });
    return;
  }
  const id = input.dataset.addressMonitor ?? input.dataset.addressNotify;
  const entry = session?.addresses.find((saved) => saved.id === id);
  if (entry) {
    input.disabled = true;
    void setAddressOptions(
      entry.id,
      input.dataset.addressMonitor ? input.checked : entry.monitoringEnabled,
      input.dataset.addressNotify ? input.checked : entry.notificationsEnabled,
    )
      .then((value) => {
        session = value;
        render();
      })
      .catch(() => {
        input.disabled = false;
        input.checked = !input.checked;
        showDialog(uk.addresses, `<p>${uk.errors.storage_error}</p>`);
      });
  }
  if (select.id === "history-address") void loadArchive(select.value, null);
  if (select.id === "change-date") {
    changeDate = select.value;
    render();
  }
});

function applyTheme(): void {
  const theme = localStorage.getItem("theme") ?? "system";
  document.documentElement.dataset.theme = theme;
  document.documentElement.dataset.fontSize =
    localStorage.getItem("font-size") ?? "normal";
}

function persistViewPreferences(): void {
  void setViewPreferences({
    theme: document.documentElement.dataset.theme as
      "system" | "light" | "dark",
    view: mode,
    fontSize: document.documentElement.dataset.fontSize as "normal" | "large",
  }).catch(() => showDialog(uk.settings, `<p>${uk.errors.storage_error}</p>`));
}

applyTheme();
render();
void Promise.all([getSession(), getRuntimeInfo(), getViewPreferences()])
  .then(([value, info, preferences]) => {
    if (preferences) {
      localStorage.setItem("theme", preferences.theme);
      localStorage.setItem("font-size", preferences.fontSize);
      localStorage.setItem("schedule-view", preferences.view);
      mode = preferences.view;
      applyTheme();
    }
    runtime = info;
    if (info.platform === "android") void refreshNotificationPermission();
    if (!preferences) persistViewPreferences();
    clockAnchor = Date.parse(info.now);
    receivedAt = performance.now();
    applySession(value);
    if (info.startInSettings) {
      page = "settings";
      render();
      void loadAbout();
    }
    if (isTauri()) void checkForUpdates(true);
  })
  .catch(() => {
    errorCode = "invalid_data";
    render();
  });
setInterval(tick, 1000);
setInterval(() => {
  if (runtime.localProjection) void updateProjection();
}, 5000);

if (isTauri()) {
  setInterval(() => void checkForUpdates(true), 3_600_000);
  void listen<number>("update-progress", ({ payload }) => {
    const progress =
      document.querySelector<HTMLProgressElement>("#update-progress");
    if (progress) progress.value = Math.max(0, Math.min(100, payload));
  });
  void listen(
    "notification-permission-changed",
    () => void refreshNotificationPermission(),
  );
  void listen("open-dashboard", () => {
    page = "home";
    render();
    void updateProjection();
  });
  void listen("open-settings", () => {
    page = "settings";
    render();
    void loadAbout();
  });
  void listen("refresh-requested", () => void load());
}

function permissionMarkup(): string {
  return runtime.platform === "android" && notificationsAllowed === false
    ? `<p class="notice" role="status">${uk.errors.notifications_denied}</p>`
    : "";
}
async function refreshNotificationPermission(): Promise<void> {
  const revision = ++permissionRevision;
  try {
    const status = await getBackgroundStatus();
    if (revision !== permissionRevision) return;
    notificationsAllowed = status.notificationsAllowed ?? null;
    updateReminderStatus(shell, status.reminderStatus);
    const notice = shell.querySelector("#notification-permission-notice");
    if (notice) notice.innerHTML = permissionMarkup();
  } catch {
    /* Native permission state will be retried when the app resumes. */
  }
}
window.addEventListener("focus", () => {
  if (runtime.platform === "android") void refreshNotificationPermission();
});
