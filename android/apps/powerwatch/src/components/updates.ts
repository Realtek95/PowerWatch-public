import { uk } from "../strings";
import { escapeHtml as html } from "../format";
import { icon } from "../icons";
import type { UpdateResult } from "../types";

export function updateAction(value?: UpdateResult): {
  label: string;
  action: string;
} {
  if (!value) return { label: uk.checkUpdates, action: "check-updates" };
  if (["downloading", "waiting"].includes(value.phase ?? ""))
    return { label: uk.updateViewDownload, action: "open-update" };
  if (value.phase === "checking")
    return { label: uk.updateChecking, action: "open-update" };
  if (value.phase === "verifying")
    return { label: uk.updateVerifying, action: "open-update" };
  if (value.phase === "installing")
    return { label: uk.updateInstalling, action: "open-update" };
  if (value.phase === "error")
    return { label: uk.updateView, action: "open-update" };
  if (value.phase === "permission_required")
    return { label: uk.updateContinue, action: "install-update" };
  if (value.phase === "installer_opened")
    return { label: uk.updateInstallAgain, action: "install-update" };
  if (value.packageState === "ready")
    return { label: uk.updateInstallReady, action: "install-update" };
  if (value.status === "available")
    return { label: uk.updateView, action: "open-update" };
  return { label: uk.checkUpdates, action: "check-updates" };
}
function description(value: UpdateResult, platform?: string): string {
  switch (value.phase) {
    case "checking":
      return uk.updateChecking;
    case "downloading":
      return uk.updateDownload;
    case "waiting":
      return uk.updateWaiting;
    case "verifying":
      return uk.updateVerifying;
    case "installing":
      return uk.updateInstalling;
    case "permission_required":
      return uk.updatePermission;
    case "installer_opened":
      return platform === "android"
        ? uk.updateInstallerOpened
        : uk.updateInstallerWindows;
    case "error":
      return (
        uk.errors[value.errorCode as keyof typeof uk.errors] ??
        uk.errors.internal_error
      );
    default:
      return value.packageState === "ready"
        ? uk.updateReady
        : uk.updateAvailable;
  }
}
export function updateBanner(value: UpdateResult, platform?: string): string {
  if (value.phase === "idle" || (!value.phase && value.status !== "available"))
    return "";
  const action = updateAction(value);
  return `<div class="update-banner" role="status"><div>${icon("refresh")}<span>${html(description(value, platform))}${["downloading", "waiting"].includes(value.phase ?? "") ? ` <b data-update-percent>${value.progress ?? 0}%</b>` : ""}</span></div><button class="text-button" data-action="${action.action}">${action.label}</button></div>`;
}
export function updateMarkup(result: UpdateResult, platform?: string): string {
  const active = [
    "checking",
    "downloading",
    "waiting",
    "verifying",
    "installing",
  ].includes(result.phase ?? "");
  if (!active && result.phase !== "error" && result.status !== "available")
    return `<p>${result.status === "current" ? uk.updateCurrent : result.status === "disabled" ? uk.updatesUnpublished : uk.updateSkipped}</p>`;
  const ready = result.packageState === "ready";
  const expired = result.errorCode === "update_expired" || !result.version;
  const action = expired
    ? "check-updates"
    : ready
      ? "install-update"
      : "download-update";
  const label = expired
    ? uk.checkUpdates
    : ready
      ? result.phase === "permission_required"
        ? uk.updateContinue
        : result.phase === "installer_opened"
          ? uk.updateInstallAgain
          : uk.updateInstallReady
      : result.packageState === "partial"
        ? uk.updateRetry
        : uk.updateInstall;
  return `${result.version ? `<p><strong>${uk.updateAvailable}: ${html(result.version)}</strong></p>` : ""}<p id="update-phase" role="status">${html(description(result, platform))}</p>${active ? `<progress id="update-progress" max="100" ${result.phase === "checking" ? "" : `value="${result.progress ?? 0}"`} aria-label="${html(description(result, platform))}"></progress>${["downloading", "waiting"].includes(result.phase ?? "") ? `<p data-update-percent>${result.progress ?? 0}%</p>` : ""}` : ""}${!active && result.version ? `<p class="muted">${uk.updateSize}: ${Math.ceil((result.size ?? 0) / 1024 / 1024)} МБ</p><p class="release-notes">${html(result.notes ?? "")}</p>${ready ? "" : `<p>${platform === "android" ? uk.updateConsentAndroid : uk.updateConsentWindows}</p>`}` : ""}<div class="form-actions">${active ? "" : `<button class="primary-button" data-action="${action}">${label}</button>`}<button class="text-button" data-action="close-dialog">${active ? uk.updateMinimize : uk.close}</button></div>`;
}
