//! Shared, local executor. Each tick performs at most one request. A persisted
//! lease serializes separate Android processes and an open desktop window.
mod network;
pub mod widget;
use chrono::{DateTime, TimeDelta, Utc};
pub use network::configure_http_policy;
use powerwatch_notifications::plan_snapshot;
use powerwatch_providers::{SnapshotSource, live::service::LiveScheduleSource};
use powerwatch_storage::{StorageError, Store, address_key};
use serde::Serialize;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TickResult {
    pub checked: Option<String>,
    pub next_delay_seconds: u64,
}
/// No catch-up loop. The next interval starts from actual completion; errors
/// retain the trusted cache and use exponential backoff (maximum six hours).
pub fn tick(store: &mut Store, wall: DateTime<Utc>) -> Result<TickResult, StorageError> {
    tick_with_source(store, wall, &LiveScheduleSource)
}
/// The source is injected independently of persistence and scheduling policy.
pub fn tick_with_source(
    store: &mut Store,
    now: DateTime<Utc>,
    source: &dyn SnapshotSource,
) -> Result<TickResult, StorageError> {
    let wall = now;
    store.cancel_invalid_notification_plans(now)?;
    store.reconcile(now)?;
    let session = store.load_book()?.state();
    let requested = store
        .setting::<String>("manual_refresh_address")?
        .filter(|id| !id.is_empty());
    let manual = requested
        .as_ref()
        .filter(|id| session.addresses.iter().any(|a| &a.id == *id));
    if requested.is_some() && manual.is_none() {
        store.set_setting("manual_refresh_address", &"")?;
    }
    let policy = if manual.is_some() {
        powerwatch_core::monitoring::MonitoringSettings {
            enabled: true,
            work_window: None,
        }
    } else {
        session.monitoring.clone()
    };
    if !session.monitoring.enabled && manual.is_none() {
        return Ok(TickResult {
            checked: None,
            next_delay_seconds: 900,
        });
    }
    let previous = store.setting("last_poll_finished")?;
    let allowed = policy.next_allowed_at(now, previous)?;
    if allowed > now {
        return Ok(TickResult {
            checked: None,
            next_delay_seconds: delay(now, allowed),
        });
    }
    let states = store.poll_states()?;
    let selected = session
        .addresses
        .iter()
        .filter(|saved| {
            if let Some(id) = manual {
                return &saved.id == id;
            }
            saved.monitoring_enabled
                && (saved.address.provider_id != "manual"
                    || saved.address.details.as_ref().is_some_and(|details| {
                        details.queue.is_some()
                            || details
                                .selection
                                .as_ref()
                                .is_some_and(|s| s.queue.is_some())
                    }))
        })
        .map(|saved| {
            let key = address_key(saved);
            let due = if manual.is_some() {
                now
            } else {
                states
                    .iter()
                    .find(|state| state.address_key == key)
                    .map(|state| state.due_at)
                    .unwrap_or(now)
            };
            (saved, key, due)
        })
        .min_by_key(|(_, _, due)| *due);
    let Some((saved, key, due)) = selected else {
        return Ok(TickResult {
            checked: None,
            next_delay_seconds: 900,
        });
    };
    if due > now {
        return Ok(TickResult {
            checked: None,
            next_delay_seconds: delay(now, due),
        });
    }
    if manual.is_some() {
        let allowed = retry_after_source_pauses(store, &saved.address, now)?;
        if allowed > now {
            return Ok(TickResult {
                checked: None,
                next_delay_seconds: delay(now, allowed),
            });
        }
    }
    let Some(lease) = store.claim_poll(wall)? else {
        return Ok(TickResult {
            checked: None,
            next_delay_seconds: 10,
        });
    };
    let result = (|| {
        let latest = store.setting("last_poll_finished")?;
        let allowed = policy.next_allowed_at(now, latest)?;
        if allowed > now {
            return Ok(TickResult {
                checked: None,
                next_delay_seconds: delay(now, allowed),
            });
        }
        let started = std::time::Instant::now();
        let previous = store.cached(&key)?;
        let fetched = if manual.is_some() {
            source.refresh_for(&saved.address, now, previous.as_ref())
        } else {
            source.snapshot_for(&saved.address, now, previous.as_ref())
        };
        let finished =
            now + TimeDelta::from_std(started.elapsed()).map_err(|_| StorageError::InvalidTime)?;
        let latest = store.load_book()?.state();
        if (!latest.monitoring.enabled && manual.is_none())
            || !latest.addresses.iter().any(|entry| {
                entry.id == saved.id
                    && address_key(entry) == key
                    && (entry.monitoring_enabled || manual.is_some())
            })
        {
            store.complete_poll(
                &key,
                finished,
                next_poll_at(finished, latest.check_interval_minutes, 0, false),
                Some("selection_changed"),
            )?;
            return Ok(TickResult {
                checked: Some(saved.id.clone()),
                next_delay_seconds: powerwatch_core::monitoring::ADDRESS_PAUSE_SECONDS as u64,
            });
        }
        let error = match fetched {
            Ok(mut snapshot) => {
                snapshot.address.label = saved.address.label.clone();
                let previous = store.cached(&key)?;
                let mut prefs = store.notification_preferences()?;
                prefs.enabled &= latest
                    .addresses
                    .iter()
                    .find(|entry| entry.id == saved.id)
                    .is_some_and(|entry| entry.notifications_enabled);
                let plans = plan_snapshot(
                    &key,
                    &saved.id,
                    &snapshot,
                    previous.as_ref(),
                    finished,
                    session.check_interval_minutes,
                    &prefs,
                )?;
                let issue = snapshot.forecast_error.clone();
                store.ingest_with_plans(&key, snapshot, finished, Some(&plans))?;
                issue
            }
            Err(error) => Some(error.code().to_owned()),
        };
        let failures = states
            .iter()
            .find(|state| state.address_key == key)
            .map(|state| state.failures)
            .unwrap_or(0);
        let due = next_poll_at(
            finished,
            session.check_interval_minutes,
            failures,
            error.is_some(),
        );
        let due = if error.is_some() {
            retry_after_source_pauses(store, &saved.address, due)?
        } else {
            due
        };
        store.complete_poll(&key, finished, due, error.as_deref())?;
        Ok(TickResult {
            checked: Some(saved.id.clone()),
            next_delay_seconds: powerwatch_core::monitoring::ADDRESS_PAUSE_SECONDS as u64,
        })
    })();
    let cleanup = (|| {
        if result.as_ref().is_ok_and(|result| result.checked.is_some())
            && let Some(id) = manual
            && store.setting::<String>("manual_refresh_address")?.as_ref() == Some(id)
        {
            store.set_setting("manual_refresh_address", &"")?;
        }
        Ok::<_, StorageError>(())
    })();
    let released = store.release_poll(&lease);
    cleanup?;
    released?;
    result
}
fn delay(now: DateTime<Utc>, due: DateTime<Utc>) -> u64 {
    (due - now).num_seconds().max(1) as u64
}
/// Shared due time for manual and automatic checks. Manual use bypasses the work
/// window, but still postpones automatic work and preserves failure backoff.
pub fn next_poll_at(
    finished: DateTime<Utc>,
    interval: u16,
    failures: u32,
    failed: bool,
) -> DateTime<Utc> {
    let minutes = if failed {
        match failures {
            0 => 1,
            1 => 3,
            _ => u32::from(interval)
                .saturating_mul(2_u32.pow((failures - 2).min(6)))
                .min(360),
        }
    } else {
        u32::from(interval)
    };
    finished + TimeDelta::minutes(i64::from(minutes))
}
/// Avoid spending recovery attempts while every applicable forecast host is paused.
/// An available backup remains eligible even if the primary host is paused.
pub fn retry_after_source_pauses(
    store: &Store,
    address: &powerwatch_core::model::Address,
    due: DateTime<Utc>,
) -> Result<DateTime<Utc>, StorageError> {
    let Some(details) = address.details.as_ref() else {
        return Ok(due);
    };
    let operator = details
        .queue
        .as_ref()
        .or_else(|| details.selection.as_ref().and_then(|s| s.queue.as_ref()))
        .map(|q| q.operator_id.as_str());
    forecast_retry_at(store, operator, due)
}
fn forecast_retry_at(
    store: &Store,
    operator: Option<&str>,
    due: DateTime<Utc>,
) -> Result<DateTime<Utc>, StorageError> {
    let Some(operator) = operator.and_then(powerwatch_providers::registry::find) else {
        return Ok(due);
    };
    let mut hosts = vec!["svitlopulse.andrewboyko.name"];
    if operator.yasno.is_some() {
        hosts.push("app.yasno.ua");
    }
    if operator.mirror_region.is_some() {
        hosts.push("raw.githubusercontent.com");
    }
    let mut earliest = None;
    for host in hosts {
        let Some(until) = store.http_pause(host)? else {
            return Ok(due);
        };
        earliest = Some(earliest.map_or(until, |value: DateTime<Utc>| value.min(until)));
    }
    Ok(earliest.map_or(due, |until| due.max(until)))
}
/// A manual action is accepted once: either execute now or leave a durable
/// request for the shared executor. Waiting for another owner is not a failure.
pub fn claim_manual_poll(
    store: &mut Store,
    saved: &powerwatch_core::session::SavedAddress,
    now: DateTime<Utc>,
) -> Result<Option<String>, StorageError> {
    if store.manual_refresh_pending(&saved.id)? {
        return Ok(None);
    }
    let Some(lease) = store.claim_poll(now)? else {
        store.request_manual_refresh(&saved.id)?;
        return Ok(None);
    };
    let policy = powerwatch_core::monitoring::MonitoringSettings {
        enabled: true,
        work_window: None,
    };
    let readiness = (|| {
        let ready = policy.next_allowed_at(now, store.setting("last_poll_finished")?)?;
        Ok::<_, StorageError>(
            ready <= now && retry_after_source_pauses(store, &saved.address, now)? <= now,
        )
    })();
    match readiness {
        Ok(true) => Ok(Some(lease)),
        other => {
            store.release_poll(&lease)?;
            other?;
            store.request_manual_refresh(&saved.id)?;
            Ok(None)
        }
    }
}
