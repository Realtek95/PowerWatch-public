import { uk } from "./strings";
import type { Dashboard, Interval } from "./types";

// Older Android ICU knows the equivalent legacy zone spelling only. Calendar
// rules remain in Rust/Europe/Kyiv; this alias is solely a display compatibility.
const displayZone = (() => {
  try {
    new Intl.DateTimeFormat("uk-UA", { timeZone: "Europe/Kyiv" });
    return "Europe/Kyiv";
  } catch {
    return "Europe/Kiev";
  }
})();

const timeFormatter = new Intl.DateTimeFormat("uk-UA", {
  timeZone: displayZone,
  hour12: false,
  hour: "2-digit",
  minute: "2-digit",
  hourCycle: "h23",
});
const dateFormatter = new Intl.DateTimeFormat("uk-UA", {
  timeZone: displayZone,
  day: "numeric",
  month: "long",
});
const momentFormatter = new Intl.DateTimeFormat("uk-UA", {
  timeZone: displayZone,
  hour12: false,
  day: "2-digit",
  month: "2-digit",
  year: "numeric",
  hour: "2-digit",
  minute: "2-digit",
  hourCycle: "h23",
});

export function formatMoment(value: string | number): string {
  return momentFormatter.format(new Date(value));
}

export function formatTime(value: string | number): string {
  return timeFormatter.format(new Date(value));
}
export function formatDate(value: string): string {
  return dateFormatter.format(new Date(`${value}T12:00:00Z`));
}
export function formatDuration(seconds: number): string {
  const minutes = Math.max(0, Math.floor(seconds / 60));
  if (seconds <= 0) return `0 ${uk.minute}`;
  if (minutes === 0) return uk.lessThanMinute;
  const hours = Math.floor(minutes / 60);
  return [
    hours ? `${hours} ${uk.hour}` : "",
    minutes % 60 ? `${minutes % 60} ${uk.minute}` : "",
  ]
    .filter(Boolean)
    .join(" ");
}
export function formatRange(interval: Interval): string {
  const end = formatTime(interval.end);
  return `${formatTime(interval.start)}–${end === "00:00" ? "24:00" : end}`;
}
export function escapeHtml(value: string): string {
  return value.replace(
    /[&<>"']/g,
    (char) =>
      ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[
        char
      ] ?? char,
  );
}
export function eventCopy(
  data: Dashboard,
  now: number,
  failed = false,
): { title: string; detail: string; countdown: string | null } {
  if (failed || data.freshness.status !== "current")
    return {
      title: uk.uncertainEvent,
      detail: uk.staleHint,
      countdown: null,
    };
  const event = data.nextEvent;
  if (!event)
    return {
      title: data.nextEventUncertain ? uk.uncertainEvent : uk.noOutages,
      detail: uk.forecastNote,
      countdown: null,
    };
  const seconds = Math.max(0, Math.ceil((Date.parse(event.at) - now) / 1000));
  const startDate = dateFormatter.format(new Date(event.periodStart));
  const endDate = dateFormatter.format(new Date(event.periodEnd));
  const dateRange = startDate !== endDate ? ` · ${startDate} – ${endDate}` : "";
  const countdown = [
    Math.floor(seconds / 3600),
    Math.floor((seconds % 3600) / 60),
    seconds % 60,
  ]
    .map((n) => String(n).padStart(2, "0"))
    .join(":");
  return {
    title:
      seconds === 0
        ? uk.eventReached
        : event.kind === "outage_start"
          ? `${uk.beforeOutage} ${formatDuration(seconds)}`
          : `${uk.plannedUntil} ${formatTime(event.at)}`,
    detail: event.periodEndKnown
      ? `${formatTime(event.periodStart)}–${formatTime(event.periodEnd)}${dateRange} · ${formatDuration((Date.parse(event.periodEnd) - Date.parse(event.periodStart)) / 1000)} · ${uk.forecastNote.toLowerCase()}`
      : `${formatTime(event.periodStart)} · ${uk.endUnknown} · ${uk.forecastNote.toLowerCase()}`,
    countdown,
  };
}
