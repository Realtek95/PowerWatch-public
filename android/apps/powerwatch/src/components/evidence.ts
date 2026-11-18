import type { Dashboard } from "../types";
import { uk } from "../strings";
import { escapeHtml as html, formatMoment } from "../format";

export function evidenceMarkup(value: Dashboard): string {
  const parts = value.snapshot.provenance ?? [];
  const start = value.bands[0]?.start;
  const end = value.bands[value.bands.length - 1]?.end;
  const entries = parts
    .filter(
      (part) =>
        !start ||
        !end ||
        (Date.parse(part.start) < Date.parse(end) &&
          Date.parse(part.end) > Date.parse(start)),
    )
    .flatMap((part) => part.evidence);
  const unique = [
    ...new Map(
      entries.map((proof) => [
        `${proof.provider.id}:${proof.publishedAt}:${proof.status}`,
        proof,
      ]),
    ).values(),
  ];
  if (!unique.length) return "";
  return `<section class="card source-evidence"><h3>${uk.forecastSources}</h3><ul>${unique
    .map(
      (proof) =>
        `<li><strong>${html(proof.provider.name)}</strong>${proof.publishedAt ? ` · ${uk.sourceDataTime}: ${html(formatMoment(proof.publishedAt))}` : ""}${proof.status !== "verified" ? ` · ${uk.sourceConflict}` : ""}</li>`,
    )
    .join("")}</ul></section>`;
}
