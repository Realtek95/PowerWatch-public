//! Central Ukrainian resources for local OS notifications.
pub const TEST: &str = "Тестове сповіщення PowerWatch.";
pub fn title(label: &str) -> String {
    format!("PowerWatch · {label}")
}
pub fn forecast_start(at: chrono::DateTime<chrono::Utc>) -> String {
    format!(
        "За останнім графіком початок відключення {}.",
        at.with_timezone(&powerwatch_core::time::LOCAL_ZONE)
            .format("%d.%m о %H:%M")
    )
}
pub fn forecast_end(at: chrono::DateTime<chrono::Utc>) -> String {
    format!(
        "За останнім графіком завершення відключення {}.",
        at.with_timezone(&powerwatch_core::time::LOCAL_ZONE)
            .format("%d.%m о %H:%M")
    )
}
pub const SCHEDULE_CHANGED: &str =
    "Графік змінився. Відкрийте «Зміни графіка», щоб переглянути було → стало.";
pub fn schedule_update(
    update: &crate::ForecastUpdate,
    now: chrono::DateTime<chrono::Utc>,
) -> String {
    use powerwatch_core::{history::ChangeKind, time::LOCAL_ZONE};
    if !update.published.is_empty() {
        let today = now.with_timezone(&LOCAL_ZONE).date_naive();
        let dates = update
            .published
            .iter()
            .map(|date| {
                if *date == today {
                    format!("сьогодні, {}", date.format("%d.%m"))
                } else if Some(*date) == today.succ_opt() {
                    format!("завтра, {}", date.format("%d.%m"))
                } else {
                    date.format("%d.%m").to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("; ");
        return format!("З'явився графік на {dates}. Перегляньте оновлений розклад.");
    }
    if let [change] = update.changes.as_slice() {
        let range = |period: &powerwatch_core::model::ScheduleInterval| {
            format!(
                "{} {}–{}",
                period.start.with_timezone(&LOCAL_ZONE).format("%d.%m"),
                period.start.with_timezone(&LOCAL_ZONE).format("%H:%M"),
                period.end.with_timezone(&LOCAL_ZONE).format("%H:%M")
            )
        };
        return match (&change.kind, &change.before, &change.after) {
            (ChangeKind::Added, _, Some(after)) => {
                format!("Графік змінився: додано відключення {}.", range(after))
            }
            (ChangeKind::Cancelled, Some(before), _) => {
                format!("Графік змінився: скасовано відключення {}.", range(before))
            }
            (_, Some(before), Some(after)) => {
                format!("Графік змінився: {} → {}.", range(before), range(after))
            }
            _ => SCHEDULE_CHANGED.into(),
        };
    }
    SCHEDULE_CHANGED.into()
}
pub const RESTORATION_CHANGED: &str =
    "Джерело змінило очікуваний час відновлення. Перегляньте оновлені дані.";
pub fn warning(at: chrono::DateTime<chrono::Utc>) -> String {
    format!(
        "За останнім графіком відключення {}.",
        at.with_timezone(&powerwatch_core::time::LOCAL_ZONE)
            .format("%d.%m о %H:%M")
    )
}
