import { uk } from "../strings";
import {
  escapeHtml as html,
  formatDate,
  formatMoment,
  formatDuration,
  formatRange,
  formatTime,
} from "../format";
import type { ArchiveView, ChangeRecord, Dashboard } from "../types";
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
/** Show the changed boundary immediately; snapshots remain available below. */
export function changePreview(record: ChangeRecord): string {
  const pair = (before: string, after: string) =>
    `${uk.before} ${before} → ${uk.after.toLocaleLowerCase("uk-UA")} ${after}`;
  const at = (value: string, other: string) =>
    formatMoment(value).slice(0, 10) === formatMoment(other).slice(0, 10)
      ? formatTime(value)
      : formatMoment(value);
  const lines = record.changes.map((change) => {
    if (change.before && change.after) {
      const before = change.before,
        after = change.after;
      if (before.end === after.end)
        return pair(
          at(before.start, after.start),
          at(after.start, before.start),
        );
      if (before.start === after.start)
        return pair(at(before.end, after.end), at(after.end, before.end));
      const differentDate =
        formatMoment(before.start).slice(0, 10) !==
        formatMoment(after.start).slice(0, 10);
      return pair(
        `${differentDate ? formatMoment(before.start).slice(0, 10) + ", " : ""}${formatRange(before)}`,
        `${differentDate ? formatMoment(after.start).slice(0, 10) + ", " : ""}${formatRange(after)}`,
      );
    }
    if (
      change.kind === "expected_restoration" &&
      record.before.expectedRestoration &&
      record.after.expectedRestoration
    )
      return pair(
        at(record.before.expectedRestoration, record.after.expectedRestoration),
        at(record.after.expectedRestoration, record.before.expectedRestoration),
      );
    if (change.kind === "queue")
      return pair(
        record.before.address.queue ?? "—",
        record.after.address.queue ?? "—",
      );
    return `${uk.changeKinds[change.kind]}${change.before ? ` · ${formatRange(change.before)}` : ""}${change.after ? ` · ${formatRange(change.after)}` : ""}`;
  });
  return [...new Set(lines)].map((line) => `<p>${html(line)}</p>`).join("");
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
  return `<section class="card"><h2>${uk.changesTitle}</h2>${archiveSelector(view)}<div class="filter-field"><label class="field-label" for="change-date">${uk.filterDate}</label>${dateControl(filterDate, view.changes[0]?.detectedDate ?? view.days[0]?.date ?? new Date().toISOString().slice(0, 10))}</div>${records.length ? records.map((record) => `<article class="change-row"><p>${uk.detectedAt}: ${formatMoment(record.detectedAt)}${record.publishedAt ? ` · ${uk.published}: ${formatMoment(record.publishedAt)}` : ""}</p><div class="change-preview">${changePreview(record)}</div><button class="text-button" data-change="${record.id}">${uk.details}</button></article>`).join("") : `<p class="empty-content">${uk.changesEmpty}</p>`}</section>`;
}
export function changeDetails(record: ChangeRecord): string {
  const snapshot = (label: string, value: ChangeRecord["before"]) =>
    `<h3>${label}</h3><p>${html(value.address.queue ?? uk.queueUnknown)} · ${html(value.sourceStatus === "reported_available" ? uk.sourceReportedRestored : value.sourceStatus === "reported_unavailable" ? uk.sourceReportedOutage : uk.sourceUnknown)}</p>${(value.queueDays ?? value.days).map((day) => `<h4>${formatDate(day.date)}</h4><ul>${day.intervals.map((item) => `<li>${formatRange(item)} · ${uk.states[item.state]}</li>`).join("")}</ul>`).join("")}`;
  return (
    `<div class="change-preview">${changePreview(record)}</div>` +
    snapshot(uk.before, record.before) +
    snapshot(uk.after, record.after)
  );
}

/** Keep the current-address summary separate from the archive navigation. */
export function currentChangesMarkup(
  view: ArchiveView | null,
  dashboard: Dashboard,
): string {
  const selected = view?.addresses.find(
    (entry) => entry.key === view.selectedKey,
  );
  if (selected?.address.id !== dashboard.snapshot.address.id) return "";
  const day = dashboard.selectedDay;
  const relevant = (change: ChangeRecord["changes"][number]) => {
    const periods = [change.before, change.after].filter(
      (part) => part !== null,
    );
    return (
      periods.length === 0 ||
      periods.some((part) =>
        day.intervals.some(
          (interval) =>
            Date.parse(part.end) > Date.parse(interval.start) &&
            Date.parse(part.start) < Date.parse(interval.end),
        ),
      )
    );
  };
  const original = view?.changes.find(
    (item) =>
      (item.after.queueDays ?? item.after.days).some(
        (schedule) => schedule.date === day.date,
      ) && item.changes.some(relevant),
  );
  if (!original) return "";
  const record = { ...original, changes: original.changes.filter(relevant) };
  const operator = record.changes.some((part) =>
    ["source_status", "expected_restoration"].includes(part.kind),
  );
  const schedule = record.changes.some(
    (part) => !["source_status", "expected_restoration"].includes(part.kind),
  );
  const periods = record.changes.filter((part) => part.before || part.after);
  const preview =
    schedule && periods.length === 1
      ? changePreview({ ...record, changes: periods })
      : "";
  return `<section class="card current-changes" aria-label="${uk.currentChanges}"><div class="current-changes-icon">${icon("history")}</div><div class="current-changes-body"><h2>${uk.currentChanges}</h2>${schedule ? `<p>${uk.scheduleChanged}</p>` : ""}${preview ? `<div class="current-changes-preview">${preview}</div>` : ""}${operator ? `<p>${uk.operatorMessageChanged}</p>` : ""}<span class="muted">${formatMoment(record.detectedAt)}</span></div><button class="secondary-button" data-change="${record.id}">${uk.details}${icon("chevron")}</button></section>`;
}
