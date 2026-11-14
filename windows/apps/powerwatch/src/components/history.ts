import { uk } from "../strings";
import {
  escapeHtml as html,
  formatDate,
  formatMoment,
  formatDuration,
  formatRange,
} from "../format";
import type { ArchiveView, ChangeRecord } from "../types";
import { selectControl, dateControl } from "./controls";
import { icon } from "../icons";

export function archiveSelector(view: ArchiveView): string {
  return `<div class="filter-field"><label class="field-label" for="history-address">${uk.archiveAddress}</label>${selectControl(
    "history-address",
    view.addresses.map((entry) => ({
      value: entry.key,
      label: `${entry.address.label} · ${entry.address.street}, ${entry.address.house}${entry.retired ? ` · ${uk.retiredAddress}` : ""}`,
    })),
    view.selectedKey ?? "",
  )}</div>`;
}
export function archiveMarkup(
  view: ArchiveView | null,
  selectedDate: string | null,
): string {
  if (!view)
    return `<section class="card"><h2>${uk.history}</h2><p>${uk.loading}</p></section>`;
  const chosen =
    view.days.find((day) => day.date === selectedDate) ?? view.days[0];
  return `<section class="card archive-card"><h2>${uk.history}</h2><p class="muted">${uk.archiveNotice}</p>${archiveSelector(view)}<div class="archive-slider"><button type="button" class="carousel-arrow" data-slide="-1" aria-label="${uk.previousDays}">${icon("chevron", "point-left")}</button><div class="archive-dates" role="group" aria-label="${uk.history}">${view.days.map((day) => `<button data-archive-date="${day.date}" aria-pressed="${chosen?.date === day.date}"><b>${formatDate(day.date)}</b><span>${uk[day.quality]}</span></button>`).join("")}</div><button type="button" class="carousel-arrow" data-slide="1" aria-label="${uk.nextDays}">${icon("chevron")}</button></div>
    ${chosen ? `<p class="notice">${uk[chosen.quality]}${chosen.quality === "partial" ? `: ${uk.archivePartial}` : ""}</p><p class="muted">${uk.lastSuccess}: ${chosen.receivedAt ? formatMoment(chosen.receivedAt) : "—"}${chosen.publishedAt ? ` · ${uk.published}: ${formatMoment(chosen.publishedAt)}` : ""}</p>${chosen.quality === "missing" ? `<p class="empty-content">${uk.archiveMissing}</p>` : ""}` : `<p>${uk.historyEmpty}</p>`}
    </section>`;
}
export function statisticsMarkup(view: ArchiveView | null): string {
  if (!view)
    return `<section class="card"><h2>${uk.statistics}</h2><p>${uk.loading}</p></section>`;
  const totals = view.statistics;
  return `<section class="card statistics-card"><h2>${uk.statistics}</h2><p class="muted">${uk.archiveNotice}</p>${archiveSelector(view)}<dl class="diagnostic-facts"><div><dt>${uk.plannedOutage}</dt><dd>${formatDuration(totals.knownOutageSeconds)} · ${uk.knownOnly}</dd></div><div><dt>${uk.uncertainTime}</dt><dd>${formatDuration(totals.uncertainSeconds)}</dd></div><div><dt>${uk.completeAverage}</dt><dd>${totals.averageCompleteDaySeconds === null ? "—" : formatDuration(totals.averageCompleteDaySeconds)}</dd></div><div><dt>${uk.fullDays} / ${uk.partialDays} / ${uk.missingDays}</dt><dd>${totals.completeDays} / ${totals.partialDays} / ${totals.missingDays}</dd></div><div><dt>${uk.outagePeriods}</dt><dd>${totals.outageCount}</dd></div></dl><div class="week-chart">${view.days.map((day) => `<div><span>${formatDate(day.date)}</span><div class="week-bar"><i style="width:${Math.min(100, (day.knownOutageSeconds / day.durationSeconds) * 100)}%"></i></div><strong>${day.quality === "missing" ? "—" : formatDuration(day.knownOutageSeconds)}</strong><small>${uk[day.quality]}</small></div>`).join("")}</div></section>`;
}
export function changesMarkup(
  view: ArchiveView | null,
  filterDate = "",
): string {
  if (!view)
    return `<section class="card"><h2>${uk.changesTitle}</h2><p>${uk.loading}</p></section>`;
  const records = view.changes.filter(
    (record) => !filterDate || record.detectedDate === filterDate,
  );
  return `<section class="card"><h2>${uk.changesTitle}</h2>${archiveSelector(view)}<div class="filter-field"><label class="field-label" for="change-date">${uk.filterDate}</label>${dateControl(filterDate, view.changes[0]?.detectedDate ?? view.days[0]?.date ?? new Date().toISOString().slice(0, 10))}</div>${records.length ? records.map((record) => `<article class="change-row"><p>${uk.detectedAt}: ${formatMoment(record.detectedAt)}${record.publishedAt ? ` · ${uk.published}: ${formatMoment(record.publishedAt)}` : ""}</p><ul>${record.changes.map((change) => `<li>${uk.changeKinds[change.kind]}${change.before ? ` · ${formatRange(change.before)}` : ""}${change.after ? ` → ${formatRange(change.after)}` : ""}</li>`).join("")}</ul><button class="text-button" data-change="${record.id}">${uk.before} → ${uk.after}</button></article>`).join("") : `<p class="empty-content">${uk.changesEmpty}</p>`}</section>`;
}
export function changeDetails(record: ChangeRecord): string {
  const snapshot = (label: string, value: ChangeRecord["before"]) =>
    `<h3>${label}</h3><p>${html(value.address.queue ?? uk.queueUnknown)} · ${html(value.sourceStatus === "reported_available" ? uk.sourceReportedRestored : value.sourceStatus === "reported_unavailable" ? uk.sourceReportedOutage : uk.sourceUnknown)}</p>${(value.queueDays ?? value.days).map((day) => `<h4>${formatDate(day.date)}</h4><ul>${day.intervals.map((item) => `<li>${formatRange(item)} · ${uk.states[item.state]}</li>`).join("")}</ul>`).join("")}`;
  return snapshot(uk.before, record.before) + snapshot(uk.after, record.after);
}
