use super::{SourceSchedule, mirror, svitlopulse, yasno};
use crate::{
    http::{FetchError, HttpClient},
    registry,
};
use chrono::{DateTime, Utc};
use powerwatch_core::grid::Queue;
use serde::{Deserialize, Serialize};
pub struct LiveService {
    pub(crate) http: HttpClient,
    direct: super::dtek::Dtek,
    backup_checks: std::sync::Mutex<std::collections::HashMap<Queue, DateTime<Utc>>>,
    reporter: std::sync::Mutex<Option<std::sync::Arc<dyn SourceReporter>>>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceCheck {
    pub provider_id: String,
    pub operator_id: String,
    pub operator_name: String,
    pub queue: String,
    pub checked_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    pub status: String,
    pub error_code: Option<String>,
}
pub trait SourceReporter: Send + Sync {
    fn record(&self, check: &SourceCheck) -> Result<(), FetchError>;
}
pub struct LiveScheduleSource;
impl crate::SnapshotSource for LiveScheduleSource {
    fn snapshot_for(
        &self,
        address: &powerwatch_core::model::Address,
        now: DateTime<Utc>,
        previous: Option<&powerwatch_core::model::ScheduleSnapshot>,
    ) -> crate::ProviderResult<powerwatch_core::model::ScheduleSnapshot> {
        LiveService::shared()?.snapshot_with_previous(address, now, previous)
    }
    fn refresh_for(
        &self,
        address: &powerwatch_core::model::Address,
        now: DateTime<Utc>,
        previous: Option<&powerwatch_core::model::ScheduleSnapshot>,
    ) -> crate::ProviderResult<powerwatch_core::model::ScheduleSnapshot> {
        LiveService::shared()?.refresh_with_previous(address, now, previous)
    }
}
impl LiveService {
    pub fn set_source_reporter(
        &self,
        reporter: std::sync::Arc<dyn SourceReporter>,
    ) -> Result<(), FetchError> {
        *self.reporter.lock().map_err(|_| FetchError::Persistence)? = Some(reporter);
        Ok(())
    }
    pub fn set_pause_repository(
        &self,
        repository: std::sync::Arc<dyn crate::http::PauseRepository>,
    ) -> Result<(), FetchError> {
        self.http.set_pause_repository(repository)
    }
    pub fn address_streets(
        &self,
        operator: &str,
        settlement: &str,
        query: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<powerwatch_core::model::CatalogEntry>, FetchError> {
        if super::dtek::supports(operator, settlement)
            && let Ok(items) = self
                .direct
                .streets(&self.http, operator, settlement, query, now)
            && !items.is_empty()
        {
            return Ok(items);
        }
        super::address::streets(&self.http, operator, settlement, query, Utc::now())
    }
    pub fn address_houses(
        &self,
        operator: &str,
        settlement: &str,
        street: &str,
        query: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<powerwatch_core::model::CatalogEntry>, FetchError> {
        if street.starts_with("dtek:") {
            self.direct
                .houses(&self.http, operator, settlement, street, query, now)
        } else {
            super::address::houses(&self.http, operator, settlement, street, query, now)
        }
    }
    pub fn confirm_address(
        &self,
        operator: &str,
        settlement: &str,
        input: &powerwatch_core::grid::AddressInput,
        ids: (&str, &str),
        now: DateTime<Utc>,
    ) -> Result<super::address::Confirmation, FetchError> {
        if ids.0.starts_with("dtek:") {
            let verified = self.direct_verification(operator, settlement, input, now)?;
            if verified.catalog.street_id != ids.0 || verified.catalog.house_id != ids.1 {
                return Err(FetchError::InvalidData);
            }
            Ok(verified)
        } else {
            super::address::confirm(&self.http, operator, settlement, input, ids, now)
        }
    }
    pub fn resolve_address(
        &self,
        operator: &str,
        settlement: &str,
        input: &powerwatch_core::grid::AddressInput,
        now: DateTime<Utc>,
    ) -> Result<super::address::Confirmation, FetchError> {
        if super::dtek::supports(operator, settlement) {
            match self.direct_verification(operator, settlement, input, now) {
                Ok(verified) => return Ok(verified),
                Err(FetchError::ComplexConnection) => {
                    return Err(FetchError::ComplexConnection);
                }
                Err(_) => {}
            }
        }
        super::address::resolve(&self.http, operator, settlement, input, Utc::now())
    }
    fn direct_verification(
        &self,
        operator: &str,
        settlement: &str,
        input: &powerwatch_core::grid::AddressInput,
        now: DateTime<Utc>,
    ) -> Result<super::address::Confirmation, FetchError> {
        let report = self
            .direct
            .lookup(&self.http, operator, settlement, input, now)?;
        Ok(super::address::Confirmation {
            operator: registry::find(operator)
                .ok_or(FetchError::InvalidData)?
                .operator
                .clone(),
            binding: report.binding,
            catalog: report.catalog,
            queue: report.queue,
        })
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFailure {
    pub provider_id: String,
    pub code: &'static str,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchBundle {
    pub candidates: Vec<SourceSchedule>,
    pub failures: Vec<SourceFailure>,
}
impl LiveService {
    pub fn snapshot(
        &self,
        address: &powerwatch_core::model::Address,
        now: DateTime<Utc>,
    ) -> crate::ProviderResult<powerwatch_core::model::ScheduleSnapshot> {
        self.snapshot_with_previous(address, now, None)
    }
    pub fn refresh_with_previous(
        &self,
        address: &powerwatch_core::model::Address,
        now: DateTime<Utc>,
        previous: Option<&powerwatch_core::model::ScheduleSnapshot>,
    ) -> crate::ProviderResult<powerwatch_core::model::ScheduleSnapshot> {
        self.http.revalidate_forecasts()?;
        self.direct.revalidate_observations()?;
        self.snapshot_with_previous(address, now, previous)
    }
    pub fn snapshot_with_previous(
        &self,
        address: &powerwatch_core::model::Address,
        now: DateTime<Utc>,
        previous: Option<&powerwatch_core::model::ScheduleSnapshot>,
    ) -> crate::ProviderResult<powerwatch_core::model::ScheduleSnapshot> {
        let details = address
            .details
            .as_ref()
            .ok_or(crate::ProviderError::UnresolvedAddress)?;
        let queue = details
            .queue
            .as_ref()
            .or_else(|| {
                details
                    .selection
                    .as_ref()
                    .and_then(|selection| selection.queue.as_ref())
            })
            .ok_or(crate::ProviderError::UnresolvedAddress)?;
        let direct = details
            .settlement_katottg
            .as_deref()
            .filter(|settlement| super::dtek::supports(&queue.operator_id, settlement))
            .filter(|_| details.original.building.trim().is_empty())
            .map(|settlement| {
                let result = self.direct.lookup(
                    &self.http,
                    &queue.operator_id,
                    settlement,
                    &details.original,
                    now,
                );
                self.report_address(queue, &result, Utc::now())?;
                if details.queue.is_some() && matches!(&result, Err(FetchError::ComplexConnection))
                {
                    return Err(FetchError::ComplexConnection);
                }
                Ok::<_, FetchError>(
                    result
                        .ok()
                        .map(|report| powerwatch_core::grid::AddressCheck {
                            binding: report.binding,
                            catalog: report.catalog,
                            evidence: report.evidence,
                            event: report.event,
                            queue: report.queue,
                        }),
                )
            })
            .transpose()?
            .flatten();
        let direct = direct.or_else(|| retained_address_check(previous, address, queue, now));
        if details.queue.is_some() && direct.as_ref().is_some_and(|report| report.queue.is_none()) {
            return Err(crate::ProviderError::UnresolvedAddress);
        }
        let effective_queue = if details.queue.is_some() {
            direct
                .as_ref()
                .and_then(|report| report.queue.clone())
                .unwrap_or_else(|| queue.clone())
        } else {
            queue.clone()
        };
        let queue = &effective_queue;
        let bundle = self.fetch(queue, Utc::now())?;
        Self::assemble(address, queue, direct, bundle, previous, now)
    }
    fn assemble(
        address: &powerwatch_core::model::Address,
        queue: &Queue,
        direct: Option<powerwatch_core::grid::AddressCheck>,
        mut bundle: FetchBundle,
        previous: Option<&powerwatch_core::model::ScheduleSnapshot>,
        now: DateTime<Utc>,
    ) -> crate::ProviderResult<powerwatch_core::model::ScheduleSnapshot> {
        use powerwatch_core::{
            model::{ScheduleSnapshot, SourceStatus},
            sourcing::resolve_day,
            time::LOCAL_ZONE,
        };
        if bundle.candidates.is_empty() && direct.is_none() {
            return Err(crate::ProviderError::Unavailable);
        }
        let received = bundle
            .candidates
            .iter()
            .map(|candidate| candidate.evidence.checked_at)
            .chain(direct.as_ref().map(|report| report.evidence.checked_at))
            .max()
            .unwrap_or(now);
        let direct = direct.filter(|report| {
            report
                .evidence
                .usable_at(received, chrono::TimeDelta::minutes(45))
        });
        let today = received.with_timezone(&LOCAL_ZONE).date_naive();
        let tomorrow = today
            .checked_add_days(chrono::Days::new(1))
            .ok_or(powerwatch_core::CoreError::InvalidTime)?;
        let mut days = Vec::new();
        let mut provenance = Vec::new();
        let mut queue_days = Vec::new();
        let mut queue_provenance = Vec::new();
        let fallback = retained_queue_candidates(previous, address, queue);
        let fresh_count = bundle.candidates.len();
        bundle.candidates.extend(fallback);
        let mut retained = false;
        let events: Vec<_> = direct
            .as_ref()
            .and_then(|report| report.event.clone())
            .into_iter()
            .collect();
        for date in [today, tomorrow] {
            let fresh = resolve_day(
                queue,
                None,
                date,
                &bundle.candidates[..fresh_count],
                &[],
                received,
            )?;
            let mut resolved_queue =
                resolve_day(queue, None, date, &bundle.candidates, &[], received)?;
            retained |= super::retention::display_gaps(
                &mut resolved_queue,
                previous,
                address,
                queue,
                received,
            );
            retained |= fresh.day.intervals != resolved_queue.day.intervals;
            queue_days.push(resolved_queue.day);
            queue_provenance.extend(resolved_queue.provenance);
            let mut resolved = resolve_day(
                queue,
                direct.as_ref().map(|report| &report.binding).or(address
                    .details
                    .as_ref()
                    .and_then(|details| details.binding.as_ref())),
                date,
                &bundle.candidates,
                &events,
                received,
            )?;
            retained |=
                super::retention::display_gaps(&mut resolved, previous, address, queue, received);
            days.push(resolved.day);
            provenance.extend(resolved.provenance);
        }
        let emergency = bundle.candidates.iter().any(|candidate| {
            candidate.emergency_dates.contains(&today)
                && candidate.evidence.provider.kind == powerwatch_core::grid::ProviderKind::Operator
        });
        let mut observed_address = address.clone();
        if address
            .details
            .as_ref()
            .is_some_and(|details| details.queue.is_some())
            && let Some(report) = &direct
        {
            observed_address.queue = report.queue.as_ref().map(|queue| queue.code.clone());
            if let Some(details) = &mut observed_address.details {
                details.queue = report.queue.clone();
                details.binding = Some(report.binding.clone());
                details.catalog = Some(report.catalog.clone());
            }
        }
        Ok(ScheduleSnapshot {
            forecast_error: (retained || fresh_count == 0).then(|| "source_unavailable".into()),
            queue_provenance: Some(queue_provenance),
            queue_days: Some(queue_days),
            address_check: direct.clone(),
            provenance: Some(provenance),
            provider_id: address.provider_id.clone(),
            is_demo: false,
            address: observed_address,
            received_at: if retained {
                previous.map_or(received, |old| old.received_at.min(received))
            } else {
                received
            },
            published_at: None,
            source_status: direct
                .as_ref()
                .filter(|report| report.event.is_some())
                .map_or(SourceStatus::Unknown, |_| SourceStatus::ReportedUnavailable),
            reason: direct
                .as_ref()
                .and_then(|report| report.event.as_ref().and_then(|event| event.reason.clone()))
                .or_else(|| emergency.then(|| crate::text::EMERGENCY.into())),
            expected_restoration: direct.as_ref().and_then(|report| {
                report
                    .event
                    .as_ref()
                    .and_then(|event| event.expected_restoration)
            }),
            days,
        })
    }
    fn report_address(
        &self,
        queue: &Queue,
        result: &Result<super::dtek::AddressReport, FetchError>,
        checked: DateTime<Utc>,
    ) -> Result<(), FetchError> {
        let reporter = self
            .reporter
            .lock()
            .map_err(|_| FetchError::Persistence)?
            .clone();
        if let Some(reporter) = reporter {
            let operator = registry::find(&queue.operator_id).ok_or(FetchError::InvalidData)?;
            reporter.record(&SourceCheck {
                provider_id: "dtek-dnem".into(),
                operator_id: queue.operator_id.clone(),
                operator_name: operator.operator.name.clone(),
                queue: result
                    .as_ref()
                    .ok()
                    .and_then(|report| report.queue.as_ref())
                    .unwrap_or(queue)
                    .code
                    .clone(),
                checked_at: checked,
                published_at: result
                    .as_ref()
                    .ok()
                    .and_then(|report| report.evidence.published_at),
                status: if result.is_ok() { "ok" } else { "error" }.into(),
                error_code: result.as_ref().err().map(|error| error.code().into()),
            })?;
        }
        Ok(())
    }
    pub fn shared() -> Result<&'static Self, FetchError> {
        static SERVICE: std::sync::OnceLock<Result<LiveService, FetchError>> =
            std::sync::OnceLock::new();
        SERVICE
            .get_or_init(Self::new)
            .as_ref()
            .map_err(|_| FetchError::Network)
    }
    pub fn new() -> Result<Self, FetchError> {
        Ok(Self {
            http: HttpClient::new()?,
            direct: Default::default(),
            backup_checks: Default::default(),
            reporter: Default::default(),
        })
    }
    /// Ordered reads; the shared executor decides when another address is due.
    /// No address text is sent for a queue forecast.
    pub fn fetch(&self, queue: &Queue, now: DateTime<Utc>) -> Result<FetchBundle, FetchError> {
        let diagnostic = {
            let mut checks = self.backup_checks.lock().map_err(|_| FetchError::Network)?;
            let previous = checks.entry(queue.clone()).or_insert(now);
            let due = now - *previous >= chrono::TimeDelta::hours(6);
            if due || now < *previous {
                *previous = now;
            }
            due
        };
        self.fetch_ordered(queue, now, diagnostic)
    }
    /// Explicit diagnostics may inspect all backups. Normal polling only uses
    /// them for missing coverage, or a rare check at most once per six hours.
    pub fn fetch_diagnostic(
        &self,
        queue: &Queue,
        now: DateTime<Utc>,
    ) -> Result<FetchBundle, FetchError> {
        self.fetch_ordered(queue, now, true)
    }
    fn fetch_ordered(
        &self,
        queue: &Queue,
        now: DateTime<Utc>,
        diagnostic: bool,
    ) -> Result<FetchBundle, FetchError> {
        let started = std::time::Instant::now();
        let completed = || now + chrono::TimeDelta::from_std(started.elapsed()).unwrap_or_default();
        let operator = registry::find(&queue.operator_id).ok_or(FetchError::InvalidData)?;
        if queue.code.is_empty()
            || queue.code.len() > 24
            || !queue.code.chars().all(|c| c.is_ascii_digit() || c == '.')
        {
            return Err(FetchError::InvalidData);
        }
        let mut bundle = FetchBundle {
            candidates: Vec::new(),
            failures: Vec::new(),
        };
        let retain = |bundle: &mut FetchBundle,
                      provider: &str,
                      result: Result<SourceSchedule, FetchError>|
         -> Result<(), FetchError> {
            let check = SourceCheck {
                provider_id: provider.into(),
                operator_id: queue.operator_id.clone(),
                operator_name: operator.operator.name.clone(),
                queue: queue.code.clone(),
                checked_at: completed(),
                published_at: result.as_ref().ok().and_then(|s| s.evidence.published_at),
                status: match &result {
                    Ok(s)
                        if s.days.iter().any(|d| {
                            d.intervals
                                .iter()
                                .any(|i| i.state != powerwatch_core::model::ScheduleState::Unknown)
                        }) || !s.emergency_dates.is_empty() =>
                    {
                        "ok"
                    }
                    Ok(_) => "no_schedule",
                    Err(_) => "error",
                }
                .into(),
                error_code: result.as_ref().err().map(|e| e.code().into()),
            };
            let reporter = self
                .reporter
                .lock()
                .map_err(|_| FetchError::Persistence)?
                .clone();
            if let Some(reporter) = reporter {
                reporter.record(&check)?;
            }
            match result {
                Ok(value) => bundle.candidates.push(value),
                Err(error) => bundle.failures.push(SourceFailure {
                    provider_id: provider.into(),
                    code: error.code(),
                }),
            };
            Ok(())
        };
        if let Some(region) = &operator.yasno {
            let result = (|| {
                let url = format!(
                    "https://app.yasno.ua/api/blackout-service/public/shutdowns/regions/{}/dsos/{}/planned-outages",
                    region.region, region.dso
                );
                let (response, checked): (yasno::Response, _) =
                    self.http.json_observed(&url, now)?;
                yasno::parse(&response, queue, checked)
            })();
            retain(&mut bundle, "yasno", result)?;
            if !diagnostic && !needs_backup(&bundle.candidates, queue, completed()) {
                return Ok(bundle);
            }
        }
        let result = (|| {
            let (summary, summary_at): (svitlopulse::Summary, _) = self
                .http
                .json_observed("https://svitlopulse.andrewboyko.name/api/v1/summary", now)?;
            if summary.demo {
                return Err(FetchError::InvalidData);
            }
            let url = format!(
                "https://svitlopulse.andrewboyko.name/api/v1/groups/{}/{}",
                operator.svitlopulse_id, queue.code
            );
            let (response, checked): (svitlopulse::Response, _) =
                self.http.json_observed(&url, now)?;
            svitlopulse::parse(
                &summary,
                &response,
                &operator.svitlopulse_id,
                queue,
                checked.max(summary_at),
            )
        })();
        retain(&mut bundle, "svitlopulse", result)?;
        if !diagnostic && !needs_backup(&bundle.candidates, queue, completed()) {
            return Ok(bundle);
        }
        if let Some(region) = &operator.mirror_region {
            let result = (|| {
                let url = format!(
                    "https://raw.githubusercontent.com/Baskerville42/outage-data-ua/main/data/{region}.json"
                );
                let (response, checked): (mirror::Response, _) =
                    self.http.json_observed(&url, now)?;
                mirror::parse(&response, region, queue, checked)
            })();
            retain(&mut bundle, "outage-data-ua", result)?;
        }
        Ok(bundle)
    }
    pub fn groups(&self, operator_id: &str, now: DateTime<Utc>) -> Result<Vec<String>, FetchError> {
        #[derive(Deserialize)]
        struct Item {
            operator_id: String,
            group: String,
        }
        #[derive(Deserialize)]
        struct Response {
            groups: Vec<Item>,
        }
        let operator = registry::find(operator_id).ok_or(FetchError::InvalidData)?;
        let url = format!(
            "https://svitlopulse.andrewboyko.name/api/v1/oblasts/{}",
            operator.oblast_slug
        );
        let response: Response = self.http.json(&url, now)?;
        let mut groups: Vec<_> = response
            .groups
            .into_iter()
            .filter(|item| item.operator_id == operator.svitlopulse_id)
            .map(|item| item.group)
            .collect();
        groups.sort();
        groups.dedup();
        Ok(groups)
    }
}
/// Reconstruct only proven queue coverage of this unchanged address. The lower
/// preference is explicit; metadata and publication dates are never refreshed.
fn retained_queue_candidates(
    previous: Option<&powerwatch_core::model::ScheduleSnapshot>,
    address: &powerwatch_core::model::Address,
    queue: &Queue,
) -> Vec<SourceSchedule> {
    use powerwatch_core::{
        grid::SourceScope,
        model::{ScheduleInterval, normalize_day},
    };
    let Some(previous) = previous.filter(|old| {
        old.address.id == address.id
            && old.provider_id == address.provider_id
            && old.address.queue.as_deref() == Some(queue.code.as_str())
    }) else {
        return Vec::new();
    };
    let days = previous.queue_days.as_ref().unwrap_or(&previous.days);
    let Some(provenance) = previous
        .queue_provenance
        .as_ref()
        .or(previous.provenance.as_ref())
    else {
        return Vec::new();
    };
    type Parts = std::collections::BTreeMap<chrono::NaiveDate, Vec<ScheduleInterval>>;
    let mut grouped: std::collections::BTreeMap<
        String,
        (powerwatch_core::grid::SourceEvidence, Parts),
    > = Default::default();
    for part in provenance {
        for proof in &part.evidence {
            if proof.scope != SourceScope::Queue(queue.clone()) {
                continue;
            }
            let Ok(key) = serde_json::to_string(proof) else {
                continue;
            };
            let (_, parts) = grouped
                .entry(key)
                .or_insert_with(|| (proof.clone(), Default::default()));
            for day in days {
                for item in &day.intervals {
                    let start = item.start.max(part.start);
                    let end = item.end.min(part.end);
                    if start < end {
                        parts.entry(day.date).or_default().push(ScheduleInterval {
                            start,
                            end,
                            state: item.state,
                        });
                    }
                }
            }
        }
    }
    let mut result = Vec::new();
    for (_, (evidence, parts)) in grouped {
        let days: Vec<_> = parts
            .into_iter()
            .filter_map(|(date, parts)| normalize_day(date, &parts).ok())
            .collect();
        if !days.is_empty() {
            result.push(SourceSchedule {
                retained: true,
                days,
                evidence,
                emergency_dates: Vec::new(),
            });
        }
    }
    result
}
/// Deliberate switching and a verified emergency are data, not missing cells.
/// A backup cannot turn those states into a confident regular forecast.
fn needs_backup(sources: &[SourceSchedule], queue: &Queue, now: DateTime<Utc>) -> bool {
    use powerwatch_core::{
        grid::SourceScope, model::ScheduleState, sourcing::resolve_day, time::LOCAL_ZONE,
    };
    let today = now.with_timezone(&LOCAL_ZONE).date_naive();
    (0..=1).any(|offset| {
        let Some(date) = today.checked_add_days(chrono::Days::new(offset)) else {
            return true;
        };
        if sources.iter().any(|s| {
            s.emergency_dates.contains(&date)
                && s.evidence.scope == SourceScope::Queue(queue.clone())
                && s.evidence.usable_at(now, chrono::TimeDelta::days(2))
        }) {
            return false;
        }
        !resolve_day(queue, None, date, sources, &[], now).is_ok_and(|day| {
            day.day
                .intervals
                .iter()
                .all(|i| i.state != ScheduleState::Unknown)
        })
    })
}
/// A stale/other-address report never suppresses a fresh queue plan.
fn retained_address_check(
    previous: Option<&powerwatch_core::model::ScheduleSnapshot>,
    address: &powerwatch_core::model::Address,
    queue: &Queue,
    now: DateTime<Utc>,
) -> Option<powerwatch_core::grid::AddressCheck> {
    let previous = previous.filter(|old| {
        old.address.id == address.id && old.address.provider_id == address.provider_id
    })?;
    let report = previous.address_check.as_ref()?;
    (report.binding.provider_id == "dtek-dnem"
        && report.evidence.provider.id == report.binding.provider_id
        && report.evidence.provider.kind == powerwatch_core::grid::ProviderKind::Operator
        && report.catalog.provider_id == report.binding.provider_id
        && report.binding.checked_at <= now
        && report.evidence.scope
            == powerwatch_core::grid::SourceScope::Address {
                operator_id: queue.operator_id.clone(),
                source_id: report.binding.source_id.clone(),
            }
        && report
            .evidence
            .usable_at(now, chrono::TimeDelta::minutes(45)))
    .then(|| report.clone())
}
