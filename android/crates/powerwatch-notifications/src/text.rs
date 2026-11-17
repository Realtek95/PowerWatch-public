//! Central Ukrainian resources for local OS notifications.
use chrono::{DateTime, NaiveDate, Timelike, Utc};
use powerwatch_core::{history::ChangeKind, model::ScheduleInterval, time::LOCAL_ZONE};
pub const TEST_TITLE: &str = "Перевірка сповіщень";
pub const TEST: &str = "Сповіщення працюють.";
/// Choose the heading and detail together for each event.
pub struct NotificationText {
    pub event: String,
    pub body: String,
}
impl NotificationText {
    fn new(event: &str, body: String) -> Self {
        Self {
            event: event.into(),
            body,
        }
    }
}
pub fn title(label: &str, event: &str) -> String {
    format!("{label} · {event}")
}
fn clock(at: DateTime<Utc>) -> String {
    at.with_timezone(&LOCAL_ZONE).format("%H:%M").to_string()
}
fn dated_time(at: DateTime<Utc>) -> String {
    at.with_timezone(&LOCAL_ZONE)
        .format("%d.%m о %H:%M")
        .to_string()
}
fn forecast(at: DateTime<Utc>) -> String {
    format!("За графіком {}.", dated_time(at))
}
pub fn warning(at: DateTime<Utc>, sending_at: DateTime<Utc>) -> NotificationText {
    if at <= sending_at {
        return NotificationText::new(
            &format!("Відключення о {}", clock(at)),
            format!("За графіком о {}.", clock(at)),
        );
    }
    let remaining = at - sending_at;
    let whole_minutes = remaining.num_minutes();
    let minutes =
        (whole_minutes + i64::from(remaining > chrono::TimeDelta::minutes(whole_minutes))).max(1);
    NotificationText::new(&format!("Відключення через {minutes} хв"), forecast(at))
}
pub fn forecast_start(at: DateTime<Utc>) -> NotificationText {
    NotificationText::new("Планове відключення", forecast(at))
}
pub fn forecast_end(at: DateTime<Utc>) -> NotificationText {
    NotificationText::new("Планове відновлення", forecast(at))
}
fn period_range(period: &ScheduleInterval, include_date: bool) -> String {
    let start = period.start.with_timezone(&LOCAL_ZONE);
    let end = period.end.with_timezone(&LOCAL_ZONE);
    let finish = if Some(end.date_naive()) == start.date_naive().succ_opt()
        && end.hour() == 0
        && end.minute() == 0
        && end.second() == 0
        && end.nanosecond() == 0
    {
        "24:00".into()
    } else if end.date_naive() == start.date_naive() {
        end.format("%H:%M").to_string()
    } else {
        end.format("%d.%m %H:%M").to_string()
    };
    let prefix = if include_date {
        format!("{}, ", start.format("%d.%m"))
    } else {
        String::new()
    };
    format!("{prefix}{}–{finish}", start.format("%H:%M"))
}
fn publication(dates: &[NaiveDate], today: NaiveDate) -> NotificationText {
    if let [date] = dates {
        let event = if *date == today {
            "Графік на сьогодні"
        } else if Some(*date) == today.succ_opt() {
            "Графік на завтра"
        } else {
            "Нові графіки"
        };
        let body = if event == "Нові графіки" {
            format!("Опубліковано на {}.", date.format("%d.%m"))
        } else {
            format!("Розклад на {} опубліковано.", date.format("%d.%m"))
        };
        return NotificationText::new(event, body);
    }
    let dates = dates
        .iter()
        .map(|date| date.format("%d.%m").to_string())
        .collect::<Vec<_>>()
        .join(", ");
    NotificationText::new("Нові графіки", format!("Опубліковано на {dates}."))
}
pub fn schedule_update(update: &crate::ForecastUpdate, now: DateTime<Utc>) -> NotificationText {
    if !update.published.is_empty() {
        return publication(
            &update.published,
            now.with_timezone(&LOCAL_ZONE).date_naive(),
        );
    }
    if let [change] = update.changes.as_slice() {
        match (&change.kind, &change.before, &change.after) {
            (ChangeKind::Added, _, Some(after)) => {
                return NotificationText::new(
                    "Додано відключення",
                    format!("{}.", period_range(after, true)),
                );
            }
            (ChangeKind::Cancelled, Some(before), _) => {
                return NotificationText::new(
                    "Відключення скасовано",
                    format!("{}.", period_range(before, true)),
                );
            }
            (_, Some(before), Some(after))
                if before.start != after.start || before.end != after.end =>
            {
                let same_date = before.start.with_timezone(&LOCAL_ZONE).date_naive()
                    == after.start.with_timezone(&LOCAL_ZONE).date_naive();
                return NotificationText::new(
                    "Час відключення змінено",
                    format!(
                        "{} → {}.",
                        period_range(before, true),
                        period_range(after, !same_date)
                    ),
                );
            }
            _ => {}
        }
    }
    NotificationText::new("Графік оновлено", "Є зміни. Перегляньте розклад.".into())
}
pub fn restoration_changed(before: DateTime<Utc>, after: DateTime<Utc>) -> NotificationText {
    let different_date = before.with_timezone(&LOCAL_ZONE).date_naive()
        != after.with_timezone(&LOCAL_ZONE).date_naive();
    let format_time = |at| {
        if different_date {
            dated_time(at)
        } else {
            clock(at)
        }
    };
    NotificationText::new(
        "Час відновлення змінено",
        format!(
            "Було {} → стало {}.",
            format_time(before),
            format_time(after)
        ),
    )
}
