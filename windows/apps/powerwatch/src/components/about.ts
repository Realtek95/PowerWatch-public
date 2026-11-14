import { uk } from "../strings";
import { escapeHtml as html, formatMoment } from "../format";
import { icon } from "../icons";
import { toggle } from "./controls";
import type { AppInfo } from "../types";

export function aboutMarkup(info: AppInfo | null, error = false): string {
  if (!info)
    return `<section class="about-section"><h3>${uk.about}</h3><p class="muted">${error ? uk.errors.internal_error : uk.loadingAppInfo}</p>${error ? `<button class="text-button" data-action="reload-about">${uk.retry}</button>` : ""}</section>`;
  const fact = (label: string, value: string) =>
    `<div><dt>${label}</dt><dd>${html(value)}</dd></div>`;
  const link = (label: string, action: string) =>
    `<button class="settings-link" data-action="${action}">${label}${icon("chevron")}</button>`;
  return `<section class="about-section" aria-labelledby="about-title"><h3 id="about-title">${uk.about}</h3>
    <div class="about-brand"><span class="brand-icon">${icon("zap")}</span><div><strong>${html(info.name)}</strong><span>${uk.author}: ${html(info.author)}</span></div></div>
    <dl class="diagnostic-facts about-facts">${fact(uk.version, info.version)}${fact(uk.buildNumber, info.buildNumber)}${fact(uk.platform, info.platform === "windows" ? "Windows" : info.platform === "android" ? "Android" : "—")}</dl>
    <div class="about-updates"><button class="secondary-button" data-action="check-updates" ${info.updatesConfigured ? "" : "disabled"}>${uk.checkUpdates}</button>${toggle(uk.automaticUpdates, info.automaticUpdates, `id="automatic-updates" ${info.updatesConfigured ? "" : "disabled"}`)}<p class="muted">${uk.lastUpdateCheck}: ${info.lastUpdateCheck ? html(formatMoment(info.lastUpdateCheck)) : uk.never}</p>${info.updatesConfigured ? "" : `<p class="muted">${uk.updatesUnpublished}</p>`}</div>
    ${info.projectUrl ? `<button class="settings-link" data-link="project">${uk.projectPage}${icon("chevron")}</button>` : ""}
    ${link(uk.dataSources, "about-sources")}${link(uk.privacy, "about-privacy")}<button class="settings-link" data-page="diagnostics">${uk.diagnosticsTitle}${icon("chevron")}</button>
    <p class="about-notice">${uk.nonOfficial}</p></section>`;
}
export function sourcesMarkup(info: AppInfo): string {
  const sources = [
    ["dtek-dnem", uk.dtekSource],
    ["yasno", "YASNO"],
    ["svitlopulse", "SvitloPulse"],
    ["outage-data-ua", "outage-data-ua"],
  ] as const;
  return `<p class="muted">${uk.sourceForecastScope}</p><div class="source-list">${sources
    .map(
      ([id, name]) =>
        `<section class="source-entry"><button class="text-button" data-link="${id}">${name}</button>${
          info.sourceChecks
            .filter((c) => c.providerId === id)
            .map(
              (c) =>
                `<div class="source-check"><strong>${html(c.operatorName)} · ${uk.group} ${html(c.queue)}</strong><span>${uk.sourceCheckStatuses[c.status] ?? uk.sourceCheckStatuses.error}</span><small>${uk.lastAttempt}: ${html(formatMoment(c.checkedAt))}${c.publishedAt ? ` · ${uk.published}: ${html(formatMoment(c.publishedAt))}` : ""}</small>${c.errorCode ? `<p class="muted">${html(uk.errors[c.errorCode as keyof typeof uk.errors] ?? uk.errors.source_unavailable)}</p>` : ""}${c.httpStatus ? `<small>${uk.sourceHttpStatus}: HTTP ${c.httpStatus}</small>` : ""}</div>`,
            )
            .join("") || `<p class="muted">${uk.sourceChecksEmpty}</p>`
        }</section>`,
    )
    .join("")}</div><h3>${uk.addressCatalog}</h3><div class="source-links">${[
    ["katottg", "КАТОТТГ"],
    ["nais", "НАІС"],
    ["drv", "ЦВК / ДРВ"],
    ["osm", "© OpenStreetMap contributors · ODbL 1.0"],
  ]
    .map(
      ([id, label]) =>
        `<button class="text-button" data-link="${id}">${label}</button>`,
    )
    .join("")}</div>`;
}
export function privacyMarkup(): string {
  return `<div class="privacy-copy"><p>${uk.privacyLocal}</p><p>${uk.privacyRequests}</p><p>${uk.privacyAnalytics}</p></div>`;
}
