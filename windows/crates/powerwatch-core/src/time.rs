use crate::CoreError;
use chrono::{DateTime, Days, NaiveDate, TimeZone, Utc};
use chrono_tz::{Europe::Kyiv, Tz};
pub const LOCAL_ZONE: Tz = Kyiv;
/// Minute 1440 means next calendar midnight, never "start + 24 hours".
/// Ambiguous/nonexistent local times require explicit source offsets instead.
pub fn local_instant(date: NaiveDate, minute: u16) -> Result<DateTime<Utc>, CoreError> {
    if minute > 1440 {
        return Err(CoreError::InvalidTime);
    }
    let (date, minute) = if minute == 1440 {
        (
            date.checked_add_days(Days::new(1))
                .ok_or(CoreError::InvalidTime)?,
            0,
        )
    } else {
        (date, minute)
    };
    let wall = date
        .and_hms_opt(u32::from(minute / 60), u32::from(minute % 60), 0)
        .ok_or(CoreError::InvalidTime)?;
    LOCAL_ZONE
        .from_local_datetime(&wall)
        .single()
        .map(|value| value.with_timezone(&Utc))
        .ok_or(CoreError::InvalidTime)
}
/// Calendar boundaries correctly encompass 23- and 25-hour local days.
pub fn day_bounds(date: NaiveDate) -> Result<(DateTime<Utc>, DateTime<Utc>), CoreError> {
    Ok((local_instant(date, 0)?, local_instant(date, 1440)?))
}
