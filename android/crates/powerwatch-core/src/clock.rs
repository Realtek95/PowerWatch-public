use chrono::{DateTime, Utc};
/// Clock contract permits deterministic tests without changing the OS clock.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}
/// Wall clock used by a regular installation.
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}
