//! Pure local notification planning. OS acceptance is not delivery confirmation.
mod revision;
pub mod text;
use chrono::{DateTime, TimeDelta, Timelike, Utc};
use powerwatch_core::{
    cache::{Freshness, FreshnessStatus},
    history::outage_periods,
    model::{ScheduleSnapshot, ScheduleState, SourceStatus},
    monitoring::WorkWindow,
    time::LOCAL_ZONE,
};
pub use revision::{ForecastReference, ForecastUpdate};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct NotificationPreferences {
    pub enabled: bool,
    pub lead_minutes: Vec<u16>,
    pub forecast_start: bool,
    pub forecast_end: bool,
    pub schedule_changes: bool,
    pub source_status: bool,
    pub restoration_changes: bool,
    pub source_trouble: bool,
    pub quiet_hours: Option<WorkWindow>,
    pub sound: bool,
    pub vibration: bool,
}
impl Default for NotificationPreferences {
    fn default() -> Self {
        Self {
            enabled: true,
            lead_minutes: vec![30, 5],
            forecast_start: true,
            forecast_end: true,
            schedule_changes: true,
            source_status: false,
            restoration_changes: true,
            source_trouble: false,
            quiet_hours: None,
            sound: true,
            vibration: true,
        }
    }
}
impl NotificationPreferences {
    pub fn allows(&self, kind: NotificationKind) -> bool {
        self.enabled
            && match kind {
                NotificationKind::Warning => !self.lead_minutes.is_empty(),
                NotificationKind::ForecastStart => self.forecast_start,
                NotificationKind::ForecastEnd => self.forecast_end,
                NotificationKind::ScheduleChanged => self.schedule_changes,
                NotificationKind::SourceOutage
                | NotificationKind::SourceRestored
                | NotificationKind::SourceCleared => false,
                NotificationKind::RestorationChanged => self.restoration_changes,
                NotificationKind::SourceTrouble => self.source_trouble,
                NotificationKind::Test => true,
            }
    }
    pub fn allows_plan(&self, plan: &PlannedNotification) -> bool {
        self.allows(plan.kind)
            && (plan.kind != NotificationKind::Warning
                || self
                    .lead_minutes
                    .iter()
                    .any(|lead| plan.key.ends_with(&format!(":{lead}"))))
    }
    pub fn is_valid(&self) -> bool {
        self.lead_minutes
            .iter()
            .all(|value| [60, 30, 15, 10, 5].contains(value))
            && self.lead_minutes.len() <= 5
            && self.quiet_hours.as_ref().is_none_or(|window| {
                window.start_minute < 1440
                    && window.end_minute < 1440
                    && window.start_minute != window.end_minute
            })
    }
    pub fn is_quiet(&self, at: DateTime<Utc>) -> bool {
        let Some(window) = &self.quiet_hours else {
            return false;
        };
        let local = at.with_timezone(&LOCAL_ZONE);
        let minute = (local.hour() * 60 + local.minute()) as u16;
        if window.start_minute < window.end_minute {
            window.start_minute <= minute && minute < window.end_minute
        } else {
            minute >= window.start_minute || minute < window.end_minute
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationKind {
    Warning,
    ForecastStart,
    ForecastEnd,
    ScheduleChanged,
    RestorationChanged,
    SourceOutage,
    SourceRestored,
    SourceCleared,
    SourceTrouble,
    Test,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedNotification {
    pub key: String,
    pub address_key: String,
    pub local_address_id: String,
    pub kind: NotificationKind,
    pub at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub title: String,
    pub body: String,
    pub sound: bool,
    pub vibration: bool,
    pub boundary: Option<DateTime<Utc>>,
}
/// An explicitly published operator calendar remains a usable reminder basis
/// during the configured HTTP pause. Aggregator/address freshness is not extended.
fn forecast_usable(snapshot: &ScheduleSnapshot, now: DateTime<Utc>, interval: u16) -> bool {
    if Freshness::for_forecast(snapshot, now, interval).status == FreshnessStatus::Current {
        return true;
    }
    if snapshot.received_at > now {
        return false;
    }
    snapshot
        .queue_provenance
        .as_ref()
        .or(snapshot.provenance.as_ref())
        .is_some_and(|parts| {
            !parts.is_empty()
                && parts.iter().all(|part| {
                    !part.evidence.is_empty()
                        && part.evidence.iter().all(|proof| {
                            proof.provider.kind == powerwatch_core::grid::ProviderKind::Operator
                                && matches!(
                                    & proof.scope,
                                    powerwatch_core::grid::SourceScope::Queue(queue) if snapshot
                                    .address.queue.as_deref() == Some(queue.code.as_str())
                                )
                                && proof.published_at.is_some()
                                && proof.valid_until.is_some()
                                && proof.usable_at(now, TimeDelta::days(2))
                        })
                })
        })
}
impl PlannedNotification {
    pub fn fresh_for(
        &self,
        snapshot: &ScheduleSnapshot,
        now: DateTime<Utc>,
        interval: u16,
    ) -> bool {
        if self.kind == NotificationKind::ScheduleChanged {
            return revision::updates_usable(snapshot, now, interval);
        }
        if matches!(
            self.kind,
            NotificationKind::SourceOutage
                | NotificationKind::SourceRestored
                | NotificationKind::SourceCleared
                | NotificationKind::RestorationChanged
        ) && let Some(check) = &snapshot.address_check
        {
            return check.evidence.usable_at(now, TimeDelta::minutes(45));
        }
        forecast_usable(snapshot, now, interval)
    }
    /// An alarm must still describe the latest known boundary. An outage ending
    /// into Unknown is never a restoration; a warning cannot cross uncertainty.
    pub fn matches_snapshot(&self, snapshot: &ScheduleSnapshot, now: DateTime<Utc>) -> bool {
        let Some(boundary) = self.boundary else {
            return match self.kind {
                NotificationKind::SourceOutage => {
                    snapshot.source_status == SourceStatus::ReportedUnavailable
                        && (!self.key.contains("|address:")
                            || snapshot
                                .address_check
                                .as_ref()
                                .and_then(|check| check.event.as_ref())
                                .is_some_and(|event| {
                                    self.key.ends_with(&format!("|address:{}", event.id))
                                }))
                }
                NotificationKind::SourceRestored => {
                    snapshot.source_status == SourceStatus::ReportedAvailable
                }
                NotificationKind::SourceCleared => {
                    snapshot.source_status == SourceStatus::Unknown
                        && snapshot.address_check.as_ref().is_some_and(|check| {
                            check.event.is_none()
                                && check.evidence.usable_at(now, TimeDelta::minutes(45))
                        })
                }
                _ => true,
            };
        };
        let periods = outage_periods(&snapshot.days);
        let intervals: Vec<_> = snapshot
            .days
            .iter()
            .flat_map(|day| &day.intervals)
            .collect();
        match self.kind {
            NotificationKind::ForecastEnd => {
                periods.iter().any(|period| period.end == boundary)
                    && intervals.iter().any(|part| {
                        part.start == boundary
                            && part.end > now
                            && part.state == ScheduleState::Available
                    })
            }
            NotificationKind::Warning => {
                boundary > now
                    && periods.iter().any(|period| period.start == boundary)
                    && !intervals.iter().any(|part| {
                        part.start < boundary
                            && part.end > now
                            && matches!(
                                part.state,
                                ScheduleState::Unknown | ScheduleState::Switching
                            )
                    })
            }
            _ => periods
                .iter()
                .any(|period| period.start == boundary && period.end > now),
        }
    }
}
fn plan(
    identity: (&str, &str),
    snapshot: &ScheduleSnapshot,
    kind: NotificationKind,
    at: DateTime<Utc>,
    suffix: &str,
    body: String,
    prefs: &NotificationPreferences,
) -> PlannedNotification {
    let (key, local_id) = identity;
    PlannedNotification {
        key: format!("{key}|{kind:?}|{suffix}"),
        address_key: key.into(),
        local_address_id: local_id.into(),
        kind,
        at,
        expires_at: at
            + match kind {
                NotificationKind::Test => TimeDelta::minutes(2),
                NotificationKind::SourceOutage
                | NotificationKind::SourceRestored
                | NotificationKind::SourceCleared => TimeDelta::minutes(45),
                _ => TimeDelta::hours(1),
            },
        title: text::title(&snapshot.address.label),
        body,
        sound: prefs.sound,
        vibration: prefs.vibration,
        boundary: None,
    }
}
/// Only future, sufficiently known boundaries get reminders. Revisions retain
/// stable interval keys; changing receipt time cannot create another warning.
pub fn plan_snapshot(
    key: &str,
    local_id: &str,
    snapshot: &ScheduleSnapshot,
    previous: Option<&ScheduleSnapshot>,
    now: DateTime<Utc>,
    interval: u16,
    prefs: &NotificationPreferences,
) -> Result<Vec<PlannedNotification>, serde_json::Error> {
    let reference = previous.map(ForecastReference::from_snapshot);
    plan_with_reference(
        key,
        local_id,
        snapshot,
        previous,
        reference.as_ref(),
        now,
        interval,
        prefs,
    )
}
#[allow(clippy::too_many_arguments)]
pub fn plan_with_reference(
    key: &str,
    local_id: &str,
    snapshot: &ScheduleSnapshot,
    previous: Option<&ScheduleSnapshot>,
    reference: Option<&ForecastReference>,
    now: DateTime<Utc>,
    interval: u16,
    prefs: &NotificationPreferences,
) -> Result<Vec<PlannedNotification>, serde_json::Error> {
    if !prefs.enabled || !prefs.is_valid() {
        return Ok(Vec::new());
    }
    let forecast_current = forecast_usable(snapshot, now, interval);
    let address_current = snapshot
        .address_check
        .as_ref()
        .is_some_and(|check| check.evidence.usable_at(now, TimeDelta::minutes(45)));
    if !forecast_current && !address_current && !revision::updates_usable(snapshot, now, interval) {
        return Ok(Vec::new());
    }
    let mut plans = Vec::new();
    let parts: Vec<_> = snapshot
        .days
        .iter()
        .flat_map(|day| &day.intervals)
        .collect();
    for outage in outage_periods(&snapshot.days)
        .into_iter()
        .filter(|_| forecast_current)
    {
        let end_known = parts
            .iter()
            .any(|part| part.start == outage.end && part.state == ScheduleState::Available);
        let uncertain_before = parts.iter().any(|part| {
            part.start < outage.start
                && part.end > now
                && matches!(
                    part.state,
                    ScheduleState::Unknown | ScheduleState::Switching
                )
        });
        let suffix = outage.start.timestamp().to_string();
        if outage.start > now && !uncertain_before {
            for &lead in &prefs.lead_minutes {
                let at = outage.start - TimeDelta::minutes(i64::from(lead));
                if at > now {
                    let mut item = plan(
                        (key, local_id),
                        snapshot,
                        NotificationKind::Warning,
                        at,
                        &format!("{suffix}:{lead}"),
                        text::warning(outage.start),
                        prefs,
                    );
                    item.boundary = Some(outage.start);
                    item.expires_at = outage.start;
                    plans.push(item);
                }
            }
            if prefs.forecast_start {
                let mut item = plan(
                    (key, local_id),
                    snapshot,
                    NotificationKind::ForecastStart,
                    outage.start,
                    &suffix,
                    text::forecast_start(outage.start),
                    prefs,
                );
                item.boundary = Some(outage.start);
                item.expires_at = (outage.start + TimeDelta::minutes(30)).min(outage.end);
                plans.push(item);
            }
        }
        if prefs.forecast_end && end_known && outage.end > now && !uncertain_before {
            let mut item = plan(
                (key, local_id),
                snapshot,
                NotificationKind::ForecastEnd,
                outage.end,
                &outage.end.timestamp().to_string(),
                text::forecast_end(outage.end),
                prefs,
            );
            item.boundary = Some(outage.end);
            item.expires_at = outage.end + TimeDelta::minutes(30);
            plans.push(item);
        }
    }
    let reference = reference
        .cloned()
        .unwrap_or_else(|| ForecastReference::from_snapshot(snapshot));
    let (next, update) = reference.advance(snapshot, now, interval);
    let version = format!("revision-{}", next.revision);
    let changed = update.is_some() && prefs.schedule_changes;
    if changed {
        plans.push(plan(
            (key, local_id),
            snapshot,
            NotificationKind::ScheduleChanged,
            now,
            &version,
            text::schedule_update(&update.unwrap(), now),
            prefs,
        ));
    }
    if let Some(old) = previous
        && (forecast_current || address_current)
        && prefs.restoration_changes
        && !changed
        && old.expected_restoration != snapshot.expected_restoration
        && (snapshot.source_status != SourceStatus::Unknown
            || snapshot.expected_restoration.is_some())
    {
        plans.push(plan(
            (key, local_id),
            snapshot,
            NotificationKind::RestorationChanged,
            now,
            &format!(
                "{:?}:{}",
                snapshot.expected_restoration,
                old.received_at.timestamp_millis()
            ),
            text::RESTORATION_CHANGED.into(),
            prefs,
        ));
    }
    plans.sort_by_key(|plan| plan.at);
    plans.dedup_by(|first, second| first.key == second.key);
    Ok(plans)
}
