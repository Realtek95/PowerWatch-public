import { uk } from "../strings";
import { escapeHtml as html, formatDuration, formatMoment } from "../format";
import type { Dashboard } from "../types";

export interface DiagnosticsState {
  data: Dashboard | null;
  lastAttempt: string | null;
  loading: boolean;
}

export function diagnosticsMarkup(state: DiagnosticsState): string {
  const { data, loading } = state;
  const row = (label: string, value: string) =>
    `<div><dt>${label}</dt><dd>${html(value)}</dd></div>`;
  return `<section class="card diagnostics-card"><div class="section-heading"><h2>${uk.diagnosticsTitle}</h2><button class="text-button" data-page="settings">${uk.back}</button></div><p class="muted">${uk.diagnosticsBody}</p>
    <dl class="diagnostic-facts">
      ${row(uk.lastAttempt, state.lastAttempt ? formatMoment(state.lastAttempt) : uk.never)}
      ${row(uk.lastSuccess, data ? formatMoment(data.snapshot.receivedAt) : uk.never)}
      ${row(uk.published, data?.snapshot.publishedAt ? formatMoment(data.snapshot.publishedAt) : "—")}
      ${row(uk.dataAge, data ? `${formatDuration(data.freshness.ageSeconds)} ${uk.ago}` : "—")}
      ${row(uk.staleAfter, data ? formatDuration(data.freshness.staleAfterSeconds) : "—")}
    </dl><button id="diagnostics-refresh" class="primary-button" data-action="refresh" ${loading ? "disabled" : ""}>${uk.refresh}</button>
  </section>`;
}

/** A provider report stays separate even when it contradicts the forecast. */
export function sourceReportMarkup(data: Dashboard): string {
  const snapshot = data.snapshot;
  if (snapshot.sourceStatus === "unknown")
    return snapshot.reason
      ? `<section class="card source-report"><h2>${uk.sourceReport}</h2><p>${html(snapshot.reason)}</p></section>`
      : "";
  const label =
    snapshot.sourceStatus === "reported_unavailable"
      ? uk.sourceReportedOutage
      : uk.sourceReportedRestored;
  const published =
    snapshot.addressCheck?.evidence.publishedAt ?? snapshot.publishedAt;
  return `<section class="card source-report"><h2>${uk.sourceReport}</h2><p><strong>${label}</strong></p>${snapshot.reason ? `<p>${html(snapshot.reason)}</p>` : ""}<p class="muted">${uk.reportedAt}: ${published ? html(formatMoment(published)) : "—"}</p>${snapshot.expectedRestoration ? `<p>${uk.expectedRestoration}: ${html(formatMoment(snapshot.expectedRestoration))}</p>` : ""}</section>`;
}
