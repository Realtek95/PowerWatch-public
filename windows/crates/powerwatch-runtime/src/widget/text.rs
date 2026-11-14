//! Central Ukrainian text for cached native projections.
use chrono::{DateTime, Utc};
use powerwatch_core::time::LOCAL_ZONE;
pub const CHOOSE_ADDRESS: &str = "Оберіть адресу в застосунку";
pub const NO_DATA: &str = "Немає даних графіка";
pub const PHYSICAL_UNKNOWN: &str = "Фактичне живлення не вимірюється";
pub const NO_EVENT: &str = "Найближчу подію не визначено";
pub const NEVER: &str = "Успішних перевірок ще немає";
pub const CHECK_FAILED: &str = "Перевірка неуспішна. Показано збережений графік.";
pub const SOURCE_OUTAGE: &str = "Джерело повідомило про відключення";
pub const SOURCE_AVAILABLE: &str = "Джерело повідомило про наявність живлення";
pub const AVAILABLE: &str = "світло";
pub const OUTAGE: &str = "відключення";
pub const SWITCHING: &str = "можливе перемикання";
pub const START: &str = "Відключення за графіком";
pub const END: &str = "Завершення за графіком";
pub const NEXT_OUTAGE: &str = "Найближче відключення:";
pub const NEXT_END: &str = "Завершення за графіком:";
pub fn day_label(now: DateTime<Utc>) -> String {
    format!(
        "Сьогодні, {}",
        now.with_timezone(&LOCAL_ZONE).format("%d.%m")
    )
}
pub fn checked(at: DateTime<Utc>, stale: bool) -> String {
    format!(
        "{}{}",
        if stale {
            "Застарілі дані · "
        } else {
            "Перевірено "
        },
        at.with_timezone(&LOCAL_ZONE).format("%d.%m %H:%M")
    )
}
pub fn forecast(state: &str, at: DateTime<Utc>) -> String {
    format!(
        "Графік на {}: {state}",
        at.with_timezone(&LOCAL_ZONE).format("%H:%M")
    )
}
pub fn event(kind: &str, at: DateTime<Utc>, uncertain: bool) -> String {
    format!(
        "{}{} · {}",
        if uncertain {
            "Невизначено: "
        } else {
            ""
        },
        kind,
        at.with_timezone(&LOCAL_ZONE).format("%d.%m %H:%M")
    )
}
pub fn event_detail(
    at: DateTime<Utc>,
    end: DateTime<Utc>,
    known_range: bool,
    now: DateTime<Utc>,
) -> String {
    let at = at.with_timezone(&LOCAL_ZONE);
    let today = now.with_timezone(&LOCAL_ZONE).date_naive();
    let date = if at.date_naive() == today {
        "Сьогодні".to_string()
    } else if at.date_naive().signed_duration_since(today).num_days() == 1 {
        "Завтра".to_string()
    } else {
        at.format("%d.%m").to_string()
    };
    if known_range {
        format!(
            "{date}, {}–{}",
            at.format("%H:%M"),
            end.with_timezone(&LOCAL_ZONE).format("%H:%M")
        )
    } else {
        format!("{date}, {}", at.format("%H:%M"))
    }
}
