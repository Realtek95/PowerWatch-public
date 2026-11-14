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
pub const RESTORATION_CHANGED: &str =
    "Джерело змінило очікуваний час відновлення. Перегляньте оновлені дані.";
pub fn warning(at: chrono::DateTime<chrono::Utc>) -> String {
    format!(
        "За останнім графіком відключення {}.",
        at.with_timezone(&powerwatch_core::time::LOCAL_ZONE)
            .format("%d.%m о %H:%M")
    )
}
