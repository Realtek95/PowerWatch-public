//! Territory, operator and source identities are independent of a particular
//! provider. Manual text never becomes evidence of an external source match.
use crate::{CoreError, model::ScheduleState};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Oblast {
    pub id: String,
    pub name: String,
    pub katottg: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Settlement {
    pub id: String,
    pub oblast_id: String,
    pub name: String,
    pub katottg: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GridOperator {
    /// Canonical operator registry ID, not a provider's region/DSO number.
    pub id: String,
    pub name: String,
    pub oblast_ids: Vec<String>,
}
/// Queue codes belong to one operator. The same visible "4.2" is not a match
/// between two operators, even if they cover the same settlement.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct Queue {
    pub operator_id: String,
    pub code: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Operator,
    Aggregator,
    Mirror,
    Synthetic,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Provider {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub website: String,
}
/// A checked address report is separate from a queue forecast and from measured
/// power. An empty outage record never means physical restoration.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddressCheck {
    pub binding: ProviderAddressId,
    pub catalog: AddressCatalogSelection,
    pub evidence: SourceEvidence,
    pub event: Option<OutageEvent>,
    pub queue: Option<Queue>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddressInput {
    pub oblast: String,
    pub settlement: String,
    pub street: String,
    pub house: String,
    pub building: String,
}
impl AddressInput {
    pub fn normalized(&self) -> Result<Self, CoreError> {
        fn field(value: &str, required: bool, limit: usize) -> Result<String, CoreError> {
            if value.chars().any(char::is_control) || value.chars().count() > limit {
                return Err(CoreError::InvalidAddress);
            }
            let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
            if required && value.is_empty() {
                return Err(CoreError::InvalidAddress);
            }
            Ok(value.to_lowercase())
        }
        Ok(Self {
            oblast: field(&self.oblast, true, 120)?,
            settlement: field(&self.settlement, true, 160)?,
            street: field(&self.street, true, 200)?,
            house: field(&self.house, true, 40)?,
            building: field(&self.building, false, 40)?,
        })
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAddressId {
    pub provider_id: String,
    pub source_id: String,
    pub checked_at: DateTime<Utc>,
}
/// A saved address can be unresolved and still be useful to its owner. An
/// operator/queue is confirmed only together with a provider binding; user
/// selections remain visibly distinct from a successful source match.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddressDetails {
    /// Geographic reference only; never used as an operator address binding.
    pub geographic: Option<GeographicAddressSelection>,
    pub catalog: Option<AddressCatalogSelection>,
    /// Explicit user choice for a queue forecast, not an address-source match.
    pub selection: Option<OperatorSelection>,
    pub original: AddressInput,
    pub normalized: AddressInput,
    pub oblast: Option<Oblast>,
    pub settlement_katottg: Option<String>,
    pub operator: Option<GridOperator>,
    pub queue: Option<Queue>,
    pub binding: Option<ProviderAddressId>,
}
impl AddressDetails {
    pub fn unresolved(input: AddressInput) -> Result<Self, CoreError> {
        Ok(Self {
            geographic: None,
            catalog: None,
            selection: None,
            normalized: input.normalized()?,
            original: input,
            oblast: None,
            settlement_katottg: None,
            operator: None,
            queue: None,
            binding: None,
        })
    }
    pub fn is_confirmed(&self) -> bool {
        match (&self.operator, &self.binding) {
            (Some(operator), Some(binding)) => {
                self.queue
                    .as_ref()
                    .is_none_or(|queue| queue.operator_id == operator.id)
                    && !operator.id.is_empty()
                    && !binding.provider_id.is_empty()
                    && !binding.source_id.is_empty()
            }
            _ => false,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperatorSelection {
    pub operator: GridOperator,
    pub queue: Option<Queue>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AddressCatalogSelection {
    pub provider_id: String,
    pub street_id: String,
    pub house_id: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GeographicAddressSelection {
    pub version: String,
    pub settlement_id: String,
    pub district_id: Option<String>,
    pub community_id: Option<String>,
    pub street_id: Option<String>,
    pub house_id: Option<String>,
    pub postcode: Option<String>,
}
/// Constructed by a provider after rechecking catalog IDs, never deserialized
/// from an IPC request as if user input were source evidence.
#[derive(Clone, Debug)]
pub struct AddressVerification {
    pub operator: GridOperator,
    pub binding: ProviderAddressId,
    pub catalog: AddressCatalogSelection,
    pub queue: Option<Queue>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    Forecast,
    RegisteredOutage,
    Emergency,
    ReportedRestoration,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum SourceScope {
    Address {
        operator_id: String,
        source_id: String,
    },
    Queue(Queue),
    Settlement(String),
    Oblast(String),
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    Verified,
    Stale,
    Conflicting,
    Unverified,
    Synthetic,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SourceEvidence {
    pub provider: Provider,
    pub scope: SourceScope,
    pub checked_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
    pub status: EvidenceStatus,
    pub content_hash: Option<String>,
}
impl SourceEvidence {
    /// Receipt time alone cannot extend the lifetime of a published forecast.
    pub fn usable_at(&self, now: DateTime<Utc>, max_age: chrono::TimeDelta) -> bool {
        let origin = self.published_at.unwrap_or(self.checked_at);
        self.status == EvidenceStatus::Verified
            && self.provider.kind != ProviderKind::Synthetic
            && origin <= self.checked_at
            && self.checked_at <= now
            && now - origin <= max_age
            && self.valid_until.is_none_or(|end| now < end)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OutageEvent {
    pub id: String,
    pub kind: EventKind,
    pub state: ScheduleState,
    pub start: DateTime<Utc>,
    /// Unknown end stays unknown; it is never a synthetic midnight restoration.
    pub end: Option<DateTime<Utc>>,
    pub expected_restoration: Option<DateTime<Utc>>,
    pub reason: Option<String>,
    pub evidence: SourceEvidence,
}
