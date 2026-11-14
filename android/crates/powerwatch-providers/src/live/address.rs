//! YASNO address catalog. Source city IDs are resolved separately from KATOTTG;
//! an ambiguous settlement name never confirms a house in another community.
use crate::{
    http::{FetchError, HttpClient},
    registry,
};
use chrono::{DateTime, Utc};
use powerwatch_core::{
    grid::{AddressCatalogSelection, AddressInput, AddressVerification, ProviderAddressId, Queue},
    model::CatalogEntry,
};
use serde::Deserialize;
#[derive(Deserialize)]
struct Entry {
    id: u64,
    value: String,
}
pub type Confirmation = AddressVerification;
struct Scope {
    source: &'static registry::YasnoRegion,
    city: Option<u64>,
}
pub(super) fn city_name(value: &str) -> String {
    let value = value.trim();
    ["м. ", "с. ", "с-ще ", "смт "]
        .iter()
        .find_map(|prefix| value.strip_prefix(prefix))
        .unwrap_or(value)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
fn choose_city(entries: &[Entry], name: &str) -> Option<u64> {
    let mut matches = entries
        .iter()
        .filter(|entry| entry.id > 0 && city_name(&entry.value) == city_name(name));
    let first = matches.next()?;
    matches.next().is_none().then_some(first.id)
}
fn street_parts(value: &str) -> (Option<&'static str>, String) {
    let normalized = value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    for (prefix, kind) in [
        ("вулиця ", "street"),
        ("вул.", "street"),
        ("площа ", "square"),
        ("пл.", "square"),
        ("проспект ", "avenue"),
        ("просп.", "avenue"),
        ("пр-т ", "avenue"),
        ("провулок ", "lane"),
        ("пров.", "lane"),
        ("бульвар ", "boulevard"),
        ("бул.", "boulevard"),
        ("узвіз ", "descent"),
        ("набережна ", "embankment"),
    ] {
        if let Some(name) = normalized.strip_prefix(prefix) {
            return (Some(kind), name.trim_start().into());
        }
    }
    (None, normalized)
}
fn same_person_name(left: &str, right: &str) -> bool {
    let left: Vec<_> = left.split_whitespace().collect();
    let right: Vec<_> = right.split_whitespace().collect();
    const GIVEN_NAMES: &[&str] = &[
        "андрія",
        "богдана",
        "василя",
        "віктора",
        "владислава",
        "володимира",
        "григорія",
        "дмитра",
        "івана",
        "іллі",
        "миколи",
        "михайла",
        "марка",
        "олександра",
        "олексія",
        "павла",
        "петра",
        "сергія",
        "степана",
        "тараса",
        "юрія",
        "лесі",
        "ліни",
        "олени",
        "ірини",
        "людмили",
        "соломії",
    ];
    left.len() == 2
        && right.len() == 2
        && left[0] == right[1]
        && left[1] == right[0]
        && left.iter().any(|word| GIVEN_NAMES.contains(word))
}
pub(super) fn same_street(left: &str, right: &str) -> bool {
    let (left_kind, left) = street_parts(left);
    let (right_kind, right) = street_parts(right);
    !left.is_empty()
        && (left == right || same_person_name(&left, &right))
        && (left_kind.is_none() || right_kind.is_none() || left_kind == right_kind)
}
/// Exact unique lookup for a manually typed or geographic-catalog address.
/// No nearest street/house or fuzzy match may create a provider binding.
pub fn resolve(
    http: &HttpClient,
    operator: &str,
    settlement: &str,
    input: &AddressInput,
    now: DateTime<Utc>,
) -> Result<Confirmation, FetchError> {
    input.normalized().map_err(|_| FetchError::InvalidData)?;
    let entries = streets(
        http,
        operator,
        settlement,
        &street_parts(&input.street).1,
        now,
    )?;
    let matches: Vec<_> = entries
        .iter()
        .filter(|entry| same_street(&entry.name, &input.street))
        .collect();
    let [street] = matches.as_slice() else {
        return Err(FetchError::InvalidData);
    };
    let entries = houses(http, operator, settlement, &street.id, &input.house, now)?;
    let matches: Vec<_> = entries
        .iter()
        .filter(|entry| entry.name.trim().eq_ignore_ascii_case(input.house.trim()))
        .collect();
    let [house] = matches.as_slice() else {
        return Err(FetchError::InvalidData);
    };
    confirm(
        http,
        operator,
        settlement,
        input,
        (&street.id, &house.id),
        now,
    )
}
fn scope(
    http: &HttpClient,
    operator_id: &str,
    settlement_id: &str,
    now: DateTime<Utc>,
) -> Result<Option<Scope>, FetchError> {
    let Some(record) = registry::find(operator_id) else {
        return Ok(None);
    };
    let Some(source) = record.yasno.as_ref() else {
        return Ok(None);
    };
    if operator_id == "ua.dso.dtek-kem" && settlement_id == "UA80000000000093317" {
        return Ok(Some(Scope { source, city: None }));
    }
    if !["ua.dso.dtek-dnem", "ua.dso.cek-dnipro"].contains(&operator_id) {
        return Ok(None);
    }
    let Some(name) = crate::catalog::unique_settlement_name("UA12000000000090473", settlement_id)
    else {
        return Ok(None);
    };
    let mut request = url(&Scope { source, city: None }, "cities")?;
    request.query_pairs_mut().append_pair("query", &name);
    let entries: Vec<Entry> = http.json(request.as_str(), now)?;
    Ok(choose_city(&entries, &name).map(|city| Scope {
        source,
        city: Some(city),
    }))
}
fn url(scope: &Scope, route: &str) -> Result<reqwest::Url, FetchError> {
    let mut url = reqwest::Url::parse(&format!(
        "https://app.yasno.ua/api/blackout-service/public/shutdowns/addresses/v2/{route}"
    ))
    .map_err(|_| FetchError::InvalidData)?;
    url.query_pairs_mut()
        .append_pair("regionId", &scope.source.region.to_string())
        .append_pair("dsoId", &scope.source.dso.to_string());
    if let Some(city) = scope.city {
        url.query_pairs_mut()
            .append_pair("cityId", &city.to_string());
    }
    Ok(url)
}
pub fn streets(
    http: &HttpClient,
    operator_id: &str,
    settlement_id: &str,
    query: &str,
    now: DateTime<Utc>,
) -> Result<Vec<CatalogEntry>, FetchError> {
    if query.trim().chars().count() < 3 {
        return Ok(Vec::new());
    }
    if query.chars().count() > 200 || query.chars().any(char::is_control) {
        return Err(FetchError::InvalidData);
    }
    let Some(scope) = scope(http, operator_id, settlement_id, now)? else {
        return Ok(Vec::new());
    };
    let mut url = url(&scope, "streets")?;
    url.query_pairs_mut().append_pair("query", query.trim());
    let entries: Vec<Entry> = http.json(url.as_str(), now)?;
    Ok(entries
        .into_iter()
        .take(100)
        .map(|entry| CatalogEntry {
            id: entry.id.to_string(),
            name: entry.value,
        })
        .collect())
}
pub fn houses(
    http: &HttpClient,
    operator_id: &str,
    settlement_id: &str,
    street_id: &str,
    query: &str,
    now: DateTime<Utc>,
) -> Result<Vec<CatalogEntry>, FetchError> {
    let Some(scope) = scope(http, operator_id, settlement_id, now)? else {
        return Ok(Vec::new());
    };
    let street: u64 = street_id.parse().map_err(|_| FetchError::InvalidData)?;
    if query.chars().count() > 40 || query.chars().any(char::is_control) {
        return Err(FetchError::InvalidData);
    }
    let mut url = url(&scope, "houses")?;
    url.query_pairs_mut()
        .append_pair("streetId", &street.to_string())
        .append_pair("query", query.trim());
    let entries: Vec<Entry> = http.json(url.as_str(), now)?;
    Ok(entries
        .into_iter()
        .take(250)
        .map(|entry| CatalogEntry {
            id: entry.id.to_string(),
            name: entry.value,
        })
        .collect())
}
pub fn confirm(
    http: &HttpClient,
    operator_id: &str,
    settlement_id: &str,
    input: &AddressInput,
    ids: (&str, &str),
    now: DateTime<Utc>,
) -> Result<Confirmation, FetchError> {
    let (street_id, house_id) = ids;
    let streets = streets(
        http,
        operator_id,
        settlement_id,
        &street_parts(&input.street).1,
        now,
    )?;
    let matches: Vec<_> = streets
        .iter()
        .filter(|item| same_street(&item.name, &input.street))
        .collect();
    let matched_street = matches.len() == 1 && matches.into_iter().any(|item| item.id == street_id);
    if !matched_street
        || !houses(
            http,
            operator_id,
            settlement_id,
            street_id,
            &input.house,
            now,
        )?
        .iter()
        .any(|item| item.id == house_id && item.name == input.house)
    {
        return Err(FetchError::InvalidData);
    }
    if !input.building.trim().is_empty() {
        return Err(FetchError::InvalidData);
    }
    #[derive(Deserialize)]
    struct Group {
        group: Option<u16>,
        subgroup: Option<u16>,
    }
    let scope = scope(http, operator_id, settlement_id, now)?.ok_or(FetchError::InvalidData)?;
    let mut url = url(&scope, "group")?;
    url.query_pairs_mut()
        .append_pair("streetId", street_id)
        .append_pair("houseId", house_id);
    let response: Group = http.json(url.as_str(), now)?;
    let record = registry::find(operator_id).ok_or(FetchError::InvalidData)?;
    let source = scope.source;
    let queue = match (response.group, response.subgroup) {
        (Some(group), Some(subgroup)) if group > 0 && subgroup > 0 => Some(Queue {
            operator_id: operator_id.into(),
            code: format!("{group}.{subgroup}"),
        }),
        (None, None) => None,
        _ => return Err(FetchError::InvalidData),
    };
    Ok(Confirmation {
        operator: record.operator.clone(),
        queue,
        catalog: AddressCatalogSelection {
            provider_id: "yasno".into(),
            street_id: street_id.into(),
            house_id: house_id.into(),
        },
        binding: ProviderAddressId {
            provider_id: "yasno".into(),
            source_id: match scope.city {
                Some(city) => {
                    format!(
                        "{}:{}:{city}:{street_id}:{house_id}",
                        source.region, source.dso
                    )
                }
                None => {
                    format!("{}:{}:{street_id}:{house_id}", source.region, source.dso)
                }
            },
            checked_at: now,
        },
    })
}
