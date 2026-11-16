import type { Dashboard } from "../types";
import { uk } from "../strings";
import { icon } from "../icons";

export function emergencyNow(data: Dashboard, now: number): boolean {
  const time = now;
  return (
    data.snapshot.planSuspensions?.some((notice) =>
      data.snapshot.days.some(
        (day) =>
          day.date === notice.date &&
          day.intervals.some(
            (part) =>
              Date.parse(part.start) <= time && time < Date.parse(part.end),
          ),
      ),
    ) ?? false
  );
}

export function statusMarkup(
  data: Dashboard,
  degraded: boolean,
  now: number,
): string {
  if (emergencyNow(data, now)) {
    const hasPlan = (data.snapshot.queueDays ?? data.snapshot.days).some(
      (day) =>
        day.intervals.some(
          (part) => Date.parse(part.start) <= now && now < Date.parse(part.end),
        ) && day.intervals.some((part) => part.state !== "unknown"),
    );
    return `<section class="status-card emergency" role="status"><span class="status-icon">${icon("warning")}</span><div><h2>${uk.emergencyTitle}</h2><p>${hasPlan ? uk.emergencyPlan : uk.emergencyNoPlan}</p></div></section>`;
  }
  return `<section class="status-card ${degraded ? "unknown" : data.currentState}"><span class="status-icon">${icon(data.currentState === "unknown" || degraded ? "info" : "zap")}</span><div><h2>${uk.statuses[data.currentState]}</h2><p>${degraded ? uk.cachedForecast : uk.forecastNote}</p></div></section>`;
}
