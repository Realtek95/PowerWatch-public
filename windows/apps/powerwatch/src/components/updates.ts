import { uk } from "../strings";
import { escapeHtml as html } from "../format";
import type { UpdateResult } from "../types";

export function updateMarkup(result: UpdateResult, platform?: string): string {
  if (result.status !== "available")
    return `<p>${result.status === "current" ? uk.updateCurrent : result.status === "disabled" ? uk.updatesUnpublished : uk.updateSkipped}</p>`;
  return `<p><strong>${uk.updateAvailable}: ${html(result.version ?? "")}</strong></p><p class="muted">${uk.updateSize}: ${Math.ceil((result.size ?? 0) / 1024 / 1024)} МБ</p><p class="release-notes">${html(result.notes ?? "")}</p><p>${platform === "android" ? uk.updateConsentAndroid : uk.updateConsentWindows}</p><div class="form-actions"><button class="primary-button" data-action="install-update">${uk.updateInstall}</button></div>`;
}
