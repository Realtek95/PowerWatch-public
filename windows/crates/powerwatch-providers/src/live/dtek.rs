//! Direct DNEM address observations using the site's ordinary CSRF session.
//! No browser cookies or challenge execution. Invalid/ambiguous data fail closed.
use crate::{
    catalog,
    http::{FetchError, HttpClient},
};
use chrono::{DateTime, NaiveDateTime, TimeDelta, TimeZone, Utc};
use powerwatch_core::{
    grid::{
        AddressCatalogSelection, AddressInput, EventKind, EvidenceStatus, OutageEvent, Provider,
        ProviderAddressId, ProviderKind, Queue, SourceEvidence, SourceScope,
    },
    model::{ScheduleState, SourceStatus},
    time::LOCAL_ZONE,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Mutex};
const OPERATOR: &str = "ua.dso.dtek-dnem";
const OBLAST: &str = "UA12000000000090473";
const WEBSITE: &str = "https://www.dtek-dnem.com.ua/ua/shutdowns";
const PROVIDER: &str = "dtek-dnem";
#[derive(Default)]
pub struct Dtek {
    session: Mutex<Option<Session>>,
}
struct Session {
    csrf: String,
    streets: BTreeMap<String, Vec<String>>,
    loaded_at: DateTime<Utc>,
    reads: BTreeMap<(String, String), (Response, DateTime<Utc>)>,
    forecast: ForecastCache,
}
#[derive(Default)]
struct ForecastCache {
    fact: Option<serde_json::Value>,
    checked_at: Option<DateTime<Utc>>,
    invalidated: bool,
}
impl ForecastCache {
    fn revision_reply(
        &mut self,
        reply: &serde_json::Value,
        checked: DateTime<Utc>,
    ) -> Result<(), FetchError> {
        let unchanged = reply.as_object().is_some_and(|object| object.len() == 1)
            && reply["result"].as_bool() == Some(false)
            && !self.revision().is_empty();
        if unchanged {
            self.checked_at = Some(checked);
            return Ok(());
        }
        if reply["result"].as_bool() != Some(true) {
            return Err(FetchError::InvalidData);
        }
        self.observe(reply, checked);
        Ok(())
    }
    fn observe(&mut self, reply: &serde_json::Value, checked: DateTime<Utc>) {
        if reply["showTableFact"].as_bool() == Some(false)
            || reply["showCurSchedule"].as_bool() == Some(false)
        {
            self.fact = None;
            self.invalidated = false;
        } else if let Some(fact) = reply.get("fact").filter(|fact| !fact.is_null()) {
            self.fact = Some(fact.clone());
            self.invalidated = false;
        }
        if !self.invalidated {
            self.checked_at = Some(checked);
        }
    }
    fn revision(&self) -> &str {
        if self.invalidated {
            return "";
        }
        self.fact
            .as_ref()
            .and_then(|fact| fact["update"].as_str())
            .unwrap_or("")
    }
}
pub struct AddressReport {
    pub binding: ProviderAddressId,
    pub catalog: AddressCatalogSelection,
    pub queue: Option<Queue>,
    pub event: Option<OutageEvent>,
    pub evidence: SourceEvidence,
    pub source_status: SourceStatus,
    pub reason: Option<String>,
    pub expected_restoration: Option<DateTime<Utc>>,
}
#[derive(Clone, Deserialize)]
struct Response {
    result: bool,
    data: BTreeMap<String, House>,
    #[serde(rename = "showCurOutageParam")]
    show_current: bool,
    #[serde(rename = "showUserGroup")]
    show_queue: bool,
    #[serde(rename = "updateTimestamp")]
    updated: String,
}
#[derive(Clone, Deserialize)]
struct House {
    sub_type: String,
    start_date: String,
    end_date: String,
    #[serde(rename = "type")]
    kind: String,
    sub_type_reason: Vec<String>,
    voluntarily: Option<bool>,
    cek: Option<bool>,
}
pub fn supports(operator: &str, settlement: &str) -> bool {
    operator == OPERATOR && catalog::unique_settlement_name(OBLAST, settlement).is_some()
}
fn source_id(parts: &[&str]) -> String {
    let bytes = serde_json::to_vec(parts).expect("string tuple serialization");
    format!("dtek:{:x}", Sha256::digest(bytes))
}
fn street_id(city: &str, street: &str) -> String {
    format!(
        "dtek:{}",
        serde_json::to_string(&[city, street]).expect("string tuple serialization")
    )
}
fn csrf_token(page: &str) -> Result<String, FetchError> {
    for part in page.split("<meta").skip(1) {
        let tag = part.split_once('>').ok_or(FetchError::InvalidData)?.0;
        if !(tag.contains("name=\"csrf-token\"") || tag.contains("name='csrf-token'")) {
            continue;
        }
        let value = tag.split_once("content=").ok_or(FetchError::InvalidData)?.1;
        let quote = value.chars().next().ok_or(FetchError::InvalidData)?;
        if !['\"', '\''].contains(&quote) {
            return Err(FetchError::InvalidData);
        }
        let token = value[1..]
            .split_once(quote)
            .ok_or(FetchError::InvalidData)?
            .0;
        if token.is_empty()
            || token.len() > 512
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_+/=".contains(&b))
        {
            return Err(FetchError::InvalidData);
        }
        return Ok(token.into());
    }
    Err(FetchError::InvalidData)
}
fn parse_session(page: &[u8], now: DateTime<Utc>) -> Result<Session, FetchError> {
    let page = std::str::from_utf8(page).map_err(|_| FetchError::InvalidData)?;
    let csrf = csrf_token(page)?;
    let json = page
        .split_once("DisconSchedule.streets = ")
        .ok_or(FetchError::InvalidData)?
        .1;
    let streets: BTreeMap<String, Vec<String>> = serde_json::Deserializer::from_str(json)
        .into_iter()
        .next()
        .ok_or(FetchError::InvalidData)?
        .map_err(|_| FetchError::InvalidData)?;
    if streets.is_empty()
        || streets
            .keys()
            .any(|city| city.len() > 500 || city.chars().any(char::is_control))
    {
        return Err(FetchError::InvalidData);
    }
    Ok(Session {
        csrf,
        streets,
        loaded_at: now,
        reads: BTreeMap::new(),
        forecast: ForecastCache {
            fact: (!page.contains("DisconSchedule.showTableFact = false")
                && !page.contains("DisconSchedule.showCurSchedule = false"))
            .then(|| {
                page.split_once("DisconSchedule.fact = ")
                    .and_then(|(_, json)| {
                        serde_json::Deserializer::from_str(json)
                            .into_iter()
                            .next()
                            .and_then(Result::ok)
                    })
            })
            .flatten(),
            checked_at: Some(now),
            invalidated: false,
        },
    })
}
impl Dtek {
    /// Reuse the fact returned with the address response. Queue-only polling
    /// uses the same revision check as the official page, never a weekly preset.
    pub fn forecast(
        &self,
        http: &HttpClient,
        queue: &Queue,
        now: DateTime<Utc>,
    ) -> Result<super::SourceSchedule, FetchError> {
        if queue.operator_id != OPERATOR {
            return Err(FetchError::InvalidData);
        }
        let result = self.read_forecast(http, queue, now);
        if matches!(result, Err(FetchError::InvalidData)) {
            http.invalid_response("www.dtek-dnem.com.ua")?;
        }
        result
    }
    fn read_forecast(
        &self,
        http: &HttpClient,
        queue: &Queue,
        now: DateTime<Utc>,
    ) -> Result<super::SourceSchedule, FetchError> {
        let now = now.max(Utc::now());
        let mut slot = self.session.lock().map_err(|_| FetchError::Network)?;
        let session = Self::session(&mut slot, http, now)?;
        if session.forecast.invalidated {
            let page = http.dtek_request(None, None, now)?;
            *session = parse_session(&page.body, page.checked_at)?;
        }
        let now = now.max(Utc::now());
        if session.forecast.invalidated
            || session
                .forecast
                .checked_at
                .is_none_or(|at| now < at || now - at >= TimeDelta::seconds(60))
        {
            let form = [
                ("method".into(), "checkDisconUpdate".into()),
                ("update".into(), session.forecast.revision().into()),
            ];
            let response = match http.dtek_request(Some(&form), Some(&session.csrf), now) {
                Ok(value) => value,
                Err(error) => {
                    if invalidates_session(&error) {
                        *slot = None;
                    }
                    return Err(error);
                }
            };
            let reply: serde_json::Value =
                serde_json::from_slice(&response.body).map_err(|_| FetchError::InvalidData)?;
            session
                .forecast
                .revision_reply(&reply, response.checked_at)?;
        }
        let checked = session.forecast.checked_at.ok_or(FetchError::InvalidData)?;
        let (days, published) = if let Some(value) =
            session.forecast.fact.as_ref().filter(|value| {
                !value["data"].as_array().is_some_and(Vec::is_empty)
                    && !value["data"]
                        .as_object()
                        .is_some_and(serde_json::Map::is_empty)
            }) {
            let fact =
                serde_json::from_value(value.clone()).map_err(|_| FetchError::InvalidData)?;
            let (days, publication, _) = super::dtek_fact::parse(&fact, queue, checked)?;
            (days, Some(publication))
        } else {
            let today = checked.with_timezone(&LOCAL_ZONE).date_naive();
            let tomorrow = today
                .checked_add_days(chrono::Days::new(1))
                .ok_or(FetchError::InvalidData)?;
            (
                [today, tomorrow]
                    .into_iter()
                    .map(|date| {
                        powerwatch_core::model::normalize_day(date, &[])
                            .map_err(|_| FetchError::InvalidData)
                    })
                    .collect::<Result<Vec<_>, _>>()?,
                None,
            )
        };
        let mut proof = super::evidence(
            OPERATOR,
            "ДТЕК Дніпровські електромережі",
            ProviderKind::Operator,
            WEBSITE,
            queue,
            checked,
            published,
        );
        proof.valid_until = Some(
            powerwatch_core::time::local_instant(days[1].date, 1440)
                .map_err(|_| FetchError::InvalidData)?,
        );
        proof.content_hash = session
            .forecast
            .fact
            .as_ref()
            .map(|fact| format!("{:x}", Sha256::digest(fact.to_string().as_bytes())));
        Ok(super::SourceSchedule {
            retained: false,
            days,
            evidence: proof,
            emergency_dates: Vec::new(),
        })
    }
    pub(super) fn revalidate_observations(&self) -> Result<(), FetchError> {
        if let Some(session) = self
            .session
            .lock()
            .map_err(|_| FetchError::Network)?
            .as_mut()
        {
            session.reads.clear();
            session.forecast.invalidated = true;
        }
        Ok(())
    }
    fn bootstrap(http: &HttpClient, now: DateTime<Utc>) -> Result<Session, FetchError> {
        let result = (|| {
            let first =
                http.dtek_request(Some(&[("method".into(), "getStreets".into())]), None, now)?;
            if first.status.as_u16() == 400 {
                let message =
                    std::str::from_utf8(&first.body).map_err(|_| FetchError::InvalidData)?;
                if !message.contains("Не вдалося перевірити передані дані")
                {
                    return Err(FetchError::InvalidData);
                }
            } else {
                let response: serde_json::Value =
                    serde_json::from_slice(&first.body).map_err(|_| FetchError::InvalidData)?;
                if response["result"].as_bool() != Some(true) {
                    return Err(FetchError::InvalidData);
                }
            }
            let page = http.dtek_request(None, None, Utc::now())?;
            parse_session(&page.body, page.checked_at)
        })();
        if matches!(&result, Err(FetchError::InvalidData)) {
            http.invalid_response("www.dtek-dnem.com.ua")?;
        }
        result
    }
    fn session<'a>(
        slot: &'a mut Option<Session>,
        http: &HttpClient,
        now: DateTime<Utc>,
    ) -> Result<&'a mut Session, FetchError> {
        if slot
            .as_ref()
            .is_none_or(|s| now < s.loaded_at || now - s.loaded_at > TimeDelta::hours(6))
        {
            *slot = Some(Self::bootstrap(http, now)?);
        }
        slot.as_mut().ok_or(FetchError::InvalidData)
    }
    fn city<'a>(
        session: &'a Session,
        settlement: &str,
    ) -> Result<(&'a str, &'a [String]), FetchError> {
        let name =
            catalog::unique_settlement_name(OBLAST, settlement).ok_or(FetchError::InvalidData)?;
        let cities: Vec<_> = session
            .streets
            .iter()
            .filter(|(city, _)| super::address::city_name(city) == super::address::city_name(&name))
            .collect();
        let [(city, streets)] = cities.as_slice() else {
            return Err(FetchError::InvalidData);
        };
        Ok((city.as_str(), streets.as_slice()))
    }
    pub fn streets(
        &self,
        http: &HttpClient,
        operator: &str,
        settlement: &str,
        query: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<powerwatch_core::model::CatalogEntry>, FetchError> {
        if !supports(operator, settlement) {
            return Ok(Vec::new());
        }
        if query.chars().any(char::is_control) || query.chars().count() > 200 {
            return Err(FetchError::InvalidData);
        }
        let query = query.trim().to_lowercase();
        if query.chars().count() < 3 {
            return Ok(Vec::new());
        }
        let mut slot = self.session.lock().map_err(|_| FetchError::Network)?;
        let session = Self::session(&mut slot, http, now)?;
        let (city, streets) = Self::city(session, settlement)?;
        Ok(streets
            .iter()
            .filter(|street| {
                !street.is_empty()
                    && street.chars().count() <= 200
                    && (street.to_lowercase().contains(&query)
                        || super::address::same_street(street, &query))
            })
            .take(100)
            .map(|street| powerwatch_core::model::CatalogEntry {
                id: street_id(city, street),
                name: street.clone(),
            })
            .collect())
    }
    fn read_street(
        &self,
        http: &HttpClient,
        operator: &str,
        settlement: &str,
        street_text: &str,
        now: DateTime<Utc>,
    ) -> Result<(String, String, Response, DateTime<Utc>), FetchError> {
        let now = now.max(Utc::now());
        if !supports(operator, settlement)
            || street_text.chars().count() > 200
            || street_text.chars().any(char::is_control)
        {
            return Err(FetchError::InvalidData);
        }
        let mut slot = self.session.lock().map_err(|_| FetchError::Network)?;
        let session = Self::session(&mut slot, http, now)?;
        let (city, streets) = Self::city(session, settlement)?;
        let matches: Vec<_> = streets
            .iter()
            .filter(|street| super::address::same_street(street, street_text))
            .collect();
        let [street] = matches.as_slice() else {
            return Err(FetchError::InvalidData);
        };
        let city = city.to_string();
        let street = street.to_string();
        let key = (city.clone(), street.clone());
        if let Some((response, checked)) = session.reads.get(&key)
            && now >= *checked
            && now - *checked < TimeDelta::seconds(60)
        {
            return Ok((city, street, response.clone(), *checked));
        }
        let form = [
            ("method".into(), "getHomeNum".into()),
            ("data[0][name]".into(), "city".into()),
            ("data[0][value]".into(), city.clone()),
            ("data[1][name]".into(), "street".into()),
            ("data[1][value]".into(), street.clone()),
            ("data[2][name]".into(), "updateFact".into()),
            ("data[2][value]".into(), session.forecast.revision().into()),
        ];
        let response = match http.dtek_request(Some(&form), Some(&session.csrf), Utc::now()) {
            Ok(value) => value,
            Err(error) => {
                if invalidates_session(&error) {
                    *slot = None;
                }
                return Err(error);
            }
        };
        let checked = response.checked_at;
        let raw: serde_json::Value = match serde_json::from_slice(&response.body) {
            Ok(raw) => raw,
            Err(_) => {
                *slot = None;
                http.invalid_response("www.dtek-dnem.com.ua")?;
                return Err(FetchError::InvalidData);
            }
        };
        match serde_json::from_slice::<Response>(&response.body) {
            Ok(response) if response.result => {
                session.forecast.observe(&raw, checked);
                if session.reads.len() >= 8
                    && !session.reads.contains_key(&key)
                    && let Some(oldest) = session
                        .reads
                        .iter()
                        .min_by_key(|(_, (_, at))| *at)
                        .map(|(key, _)| key.clone())
                {
                    session.reads.remove(&oldest);
                }
                session.reads.insert(key, (response.clone(), checked));
                Ok((city, street, response, checked))
            }
            _ => {
                *slot = None;
                http.invalid_response("www.dtek-dnem.com.ua")?;
                Err(FetchError::InvalidData)
            }
        }
    }
    pub fn houses(
        &self,
        http: &HttpClient,
        operator: &str,
        settlement: &str,
        id: &str,
        query: &str,
        now: DateTime<Utc>,
    ) -> Result<Vec<powerwatch_core::model::CatalogEntry>, FetchError> {
        if query.chars().count() > 40 || query.chars().any(char::is_control) || id.len() > 1500 {
            return Err(FetchError::InvalidData);
        }
        let keys: [String; 2] =
            serde_json::from_str(id.strip_prefix("dtek:").ok_or(FetchError::InvalidData)?)
                .map_err(|_| FetchError::InvalidData)?;
        let (city, street, response, _) =
            self.read_street(http, operator, settlement, &keys[1], now)?;
        if city != keys[0] || street != keys[1] {
            return Err(FetchError::InvalidData);
        }
        let query = query.trim().to_lowercase();
        Ok(response
            .data
            .into_keys()
            .filter(|number| {
                number.chars().count() <= 40
                    && !number.chars().any(char::is_control)
                    && number.to_lowercase().contains(&query)
            })
            .take(250)
            .map(|number| powerwatch_core::model::CatalogEntry {
                id: number.clone(),
                name: number,
            })
            .collect())
    }
    pub fn lookup(
        &self,
        http: &HttpClient,
        operator: &str,
        settlement: &str,
        input: &AddressInput,
        now: DateTime<Utc>,
    ) -> Result<AddressReport, FetchError> {
        if !input.building.trim().is_empty() {
            return Err(FetchError::InvalidData);
        }
        input.normalized().map_err(|_| FetchError::InvalidData)?;
        let (_, name) = catalog::resolve(OBLAST, settlement).ok_or(FetchError::InvalidData)?;
        if super::address::city_name(&input.settlement) != super::address::city_name(&name) {
            return Err(FetchError::InvalidData);
        }
        let (city, street, response, checked) =
            self.read_street(http, operator, settlement, &input.street, now)?;
        parse_response(response, &city, &street, &input.house, checked)
    }
}
fn invalidates_session(error: &FetchError) -> bool {
    matches!(
        error,
        FetchError::Http(400 | 401 | 403) | FetchError::InvalidData
    )
}
fn local_datetime(value: &str) -> Result<DateTime<Utc>, FetchError> {
    for format in [
        "%H:%M %d.%m.%Y",
        "%d.%m.%Y %H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%d %H:%M",
    ] {
        if let Ok(local) = NaiveDateTime::parse_from_str(value.trim(), format) {
            return LOCAL_ZONE
                .from_local_datetime(&local)
                .single()
                .map(|v| v.with_timezone(&Utc))
                .ok_or(FetchError::InvalidData);
        }
    }
    Err(FetchError::InvalidData)
}
fn parse_response(
    response: Response,
    city: &str,
    street: &str,
    house: &str,
    checked: DateTime<Utc>,
) -> Result<AddressReport, FetchError> {
    if !response.result || !response.show_current {
        return Err(FetchError::InvalidData);
    }
    let published = local_datetime(&response.updated)?;
    if published > checked || checked - published >= TimeDelta::minutes(45) {
        return Err(FetchError::InvalidData);
    }
    let matches: Vec<_> = response
        .data
        .iter()
        .filter(|(number, _)| number.trim().to_lowercase() == house.trim().to_lowercase())
        .collect();
    let [(number, record)] = matches.as_slice() else {
        return Err(FetchError::InvalidData);
    };
    if record.voluntarily == Some(true) {
        return Err(FetchError::ComplexConnection);
    }
    if record.cek == Some(true) {
        return Err(FetchError::InvalidData);
    }
    let binding = ProviderAddressId {
        provider_id: PROVIDER.into(),
        source_id: source_id(&[city, street, number]),
        checked_at: checked,
    };
    let evidence = SourceEvidence {
        provider: Provider {
            id: PROVIDER.into(),
            name: "ДТЕК Дніпровські електромережі".into(),
            kind: ProviderKind::Operator,
            website: WEBSITE.into(),
        },
        scope: SourceScope::Address {
            operator_id: OPERATOR.into(),
            source_id: binding.source_id.clone(),
        },
        checked_at: checked,
        published_at: Some(published),
        valid_until: Some(published + TimeDelta::minutes(45)),
        status: EvidenceStatus::Verified,
        content_hash: None,
    };
    let queue = if response.show_queue
        && let [code] = record.sub_type_reason.as_slice()
    {
        code.strip_prefix("GPV").and_then(|value| {
            let (group, subgroup) = value.split_once('.')?;
            if group.parse::<u16>().ok()? == 0 || subgroup.parse::<u16>().ok()? == 0 {
                return None;
            }
            Some(Queue {
                operator_id: OPERATOR.into(),
                code: value.into(),
            })
        })
    } else {
        None
    };
    let no_outage = record.kind.is_empty()
        && record.sub_type.is_empty()
        && record.start_date.is_empty()
        && record.end_date.is_empty();
    let (event, reason, expected_restoration) = if no_outage {
        (None, None, None)
    } else {
        if !["1", "2"].contains(&record.kind.as_str()) {
            return Err(FetchError::InvalidData);
        }
        let start = local_datetime(&record.start_date)?;
        if start > checked {
            return Err(FetchError::InvalidData);
        }
        let expected = if record.end_date.is_empty() {
            None
        } else {
            Some(local_datetime(&record.end_date)?)
        };
        if expected.is_some_and(|end| end <= start) {
            return Err(FetchError::InvalidData);
        }
        let reason = if record.kind == "1" {
            "Планові ремонтні роботи".to_string()
        } else {
            record.sub_type.trim().to_string()
        };
        if reason.is_empty() || reason.chars().any(char::is_control) || reason.chars().count() > 400
        {
            return Err(FetchError::InvalidData);
        }
        let event = OutageEvent {
            id: source_id(&[city, street, number, &record.start_date, &record.kind]),
            kind: if reason.to_lowercase().contains("авар") {
                EventKind::Emergency
            } else if reason.to_lowercase().contains("стабілізаційн") {
                EventKind::Forecast
            } else {
                EventKind::RegisteredOutage
            },
            state: ScheduleState::Outage,
            start,
            end: None,
            expected_restoration: expected,
            reason: Some(reason.clone()),
            evidence: evidence.clone(),
        };
        (Some(event), Some(reason), expected)
    };
    Ok(AddressReport {
        catalog: AddressCatalogSelection {
            provider_id: PROVIDER.into(),
            street_id: street_id(city, street),
            house_id: number.to_string(),
        },
        binding,
        queue,
        source_status: if no_outage {
            SourceStatus::Unknown
        } else {
            SourceStatus::ReportedUnavailable
        },
        event,
        evidence,
        reason,
        expected_restoration,
    })
}
