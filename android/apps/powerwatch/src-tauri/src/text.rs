//! Central Ukrainian resources for the native shell.
pub const OPEN: &str = "Відкрити";
pub const REFRESH: &str = "Оновити";
pub const SETTINGS: &str = "Налаштування";
pub const EXIT: &str = "Вийти";
pub const DEFAULT_TOOLTIP: &str = "PowerWatch · Дані ще не отримано";
pub const FORECAST_AVAILABLE: &str = "За графіком: світло";
pub const FORECAST_OUTAGE: &str = "За графіком: відключення";
pub const FORECAST_UNKNOWN: &str = "Прогноз невизначений";
pub fn tooltip(
    label: &str,
    status: &str,
    dashboard: &powerwatch_core::dashboard::Dashboard,
    interval: u16,
) -> String {
    let event = dashboard
        .next_event
        .as_ref()
        .map(|event| {
            format!(
                " · {}", event.at.with_timezone(& powerwatch_core::time::LOCAL_ZONE)
                .format("%H:%M")
            )
        })
        .unwrap_or_default();
    let freshness = powerwatch_core::cache::Freshness::for_snapshot(
        &dashboard.snapshot,
        dashboard.now,
        interval,
    );
    let age = if freshness.status == powerwatch_core::cache::FreshnessStatus::Current {
        format!("{} хв тому", freshness.age_seconds / 60)
    } else {
        "Застарілі дані".into()
    };
    let value = format!(
        "PowerWatch · {}\n{status}{event} · {age}", label.chars().take(24).collect::<
        String > ()
    );
    let mut length = 0;
    value
        .chars()
        .take_while(|ch| {
            length += ch.len_utf16();
            length <= 120
        })
        .collect()
}
