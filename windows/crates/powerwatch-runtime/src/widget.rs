//! Read-only projection for native surfaces. This path cannot fetch a provider or
//! freshen a receipt. Absolute timestamps remain truthful when OS updates pause.
use chrono::{DateTime, Utc};
use powerwatch_core::{
    cache::{Freshness, FreshnessStatus},
    dashboard::{EventKind, build_dashboard},
    model::{ScheduleState, SourceStatus},
    time::LOCAL_ZONE,
};
use powerwatch_storage::{StorageError, Store, address_key};
use serde::Serialize;
mod text;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetView {
    pub refreshing: bool,
    pub city: String,
    pub street: String,
    pub queue: String,
    pub operator: String,
    pub theme: String,
    pub state: ScheduleState,
    pub bands: Vec<powerwatch_core::dashboard::DayBand>,
    pub display_time: String,
    pub day_label: String,
    pub address: String,
    pub forecast: String,
    pub source: String,
    pub event: String,
    pub event_detail: String,
    pub event_caption: String,
    pub checked: String,
    pub age_seconds: Option<i64>,
    pub stale: bool,
}
/// Always includes unknown/old-data states, including after days without a check.
pub fn project(store: &Store, now: DateTime<Utc>) -> Result<WidgetView, StorageError> {
    let session = store.load_book()?.state();
    let mut view = WidgetView {
        refreshing: store
            .setting::<String>("manual_refresh_address")?
            .filter(|id| !id.is_empty())
            .as_ref()
            == session.active_address_id.as_ref()
            && session.active_address_id.is_some(),
        city: String::new(),
        street: String::new(),
        queue: String::new(),
        operator: String::new(),
        theme: store
            .setting::<serde_json::Value>("view_preferences")?
            .and_then(|v| v["theme"].as_str().map(str::to_owned))
            .unwrap_or_else(|| "system".into()),
        state: ScheduleState::Unknown,
        bands: Vec::new(),
        display_time: now.with_timezone(&LOCAL_ZONE).format("%H:%M").to_string(),
        day_label: text::day_label(now),
        address: text::CHOOSE_ADDRESS.into(),
        forecast: text::NO_DATA.into(),
        source: text::PHYSICAL_UNKNOWN.into(),
        event: text::NO_EVENT.into(),
        event_detail: text::NO_EVENT.into(),
        event_caption: String::new(),
        checked: text::NEVER.into(),
        age_seconds: None,
        stale: true,
    };
    let Some(saved) = session
        .addresses
        .iter()
        .find(|saved| Some(&saved.id) == session.active_address_id.as_ref())
    else {
        return Ok(view);
    };
    view.address = saved.address.label.clone();
    view.city = saved.address.settlement.clone();
    view.street = format!("{}, {}", saved.address.street, saved.address.house);
    view.queue = saved.address.queue.clone().unwrap_or_default();
    if let Some(details) = &saved.address.details {
        if let Some(queue) = details
            .queue
            .as_ref()
            .or_else(|| details.selection.as_ref().and_then(|s| s.queue.as_ref()))
        {
            view.queue = queue.code.clone();
        }
        if let Some(operator) = details
            .operator
            .as_ref()
            .or_else(|| details.selection.as_ref().map(|s| &s.operator))
        {
            view.operator = operator.name.clone();
        }
    }
    let Some(snapshot) = store.cached(&address_key(saved))? else {
        return Ok(view);
    };
    let freshness = Freshness::for_snapshot(&snapshot, now, session.check_interval_minutes);
    view.stale = freshness.status != FreshnessStatus::Current;
    view.age_seconds = Some(freshness.age_seconds);
    view.checked = text::checked(snapshot.received_at, view.stale);
    view.source = match snapshot.source_status {
        SourceStatus::ReportedUnavailable => text::SOURCE_OUTAGE,
        SourceStatus::ReportedAvailable => text::SOURCE_AVAILABLE,
        SourceStatus::Unknown => text::PHYSICAL_UNKNOWN,
    }
    .into();
    if store
        .poll_states()?
        .iter()
        .any(|poll| poll.address_key == address_key(saved) && poll.error_code.is_some())
    {
        view.source = text::CHECK_FAILED.into();
    }
    if let Ok(dashboard) =
        build_dashboard(snapshot, now.with_timezone(&LOCAL_ZONE).date_naive(), now)
    {
        view.state = dashboard.current_state;
        view.bands = dashboard.bands.clone();
        view.forecast = text::forecast(
            match dashboard.current_state {
                ScheduleState::Available => text::AVAILABLE,
                ScheduleState::Outage => text::OUTAGE,
                ScheduleState::Switching => text::SWITCHING,
                ScheduleState::Unknown => text::NO_DATA,
            },
            now,
        );
        if let Some(event) = dashboard.next_event {
            view.event_caption = match event.kind {
                EventKind::OutageStart => text::NEXT_OUTAGE,
                EventKind::OutageEnd => text::NEXT_END,
            }
            .into();
            view.event_detail = text::event_detail(
                event.at,
                event.period_end,
                matches!(event.kind, EventKind::OutageStart) && event.period_end_known,
                now,
            );
            if dashboard.next_event_uncertain {
                view.event_detail = text::NO_EVENT.into();
                view.event_caption.clear();
            }
            view.event = text::event(
                match event.kind {
                    EventKind::OutageStart => text::START,
                    EventKind::OutageEnd => text::END,
                },
                event.at,
                dashboard.next_event_uncertain,
            );
        }
    }
    Ok(view)
}
