//! Address selection and first-run state; persistence is added in the SQLite stage.
use crate::{
    grid::{AddressDetails, AddressInput, Oblast, OperatorSelection},
    model::Address,
    monitoring::MonitoringSettings,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedAddress {
    pub id: String,
    pub address: Address,
    #[serde(default = "enabled_by_default")]
    pub monitoring_enabled: bool,
    #[serde(default = "enabled_by_default")]
    pub notifications_enabled: bool,
}
fn enabled_by_default() -> bool {
    true
}
/// Provider IDs remain intact inside each address; local IDs identify user entries.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionState {
    pub addresses: Vec<SavedAddress>,
    pub active_address_id: Option<String>,
    pub onboarding_complete: bool,
    pub check_interval_minutes: u16,
    pub monitoring: MonitoringSettings,
}
#[derive(Debug, Error, PartialEq, Eq)]
pub enum SessionError {
    #[error("address does not exist")]
    AddressNotFound,
    #[error("the selected provider address is already saved")]
    DuplicateAddress,
    #[error("label must contain between 1 and 40 characters")]
    InvalidName,
    #[error("selection does not match the provider catalog")]
    InvalidSelection,
    #[error("no address or invalid polling interval")]
    InvalidPreferences,
    #[error("invalid monitoring window or pause")]
    InvalidMonitoring,
}
/// The only place that validates saved-address mutations for both platforms.
#[derive(Clone, Deserialize, Serialize)]
pub struct AddressBook {
    state: SessionState,
    next_id: u64,
}
impl Default for AddressBook {
    fn default() -> Self {
        Self {
            state: SessionState {
                check_interval_minutes: 30,
                ..SessionState::default()
            },
            next_id: 1,
        }
    }
}
impl AddressBook {
    pub fn confirm_address(
        &mut self,
        id: &str,
        verified: crate::grid::AddressVerification,
    ) -> Result<SessionState, SessionError> {
        let saved = self.get(id)?;
        let details = saved
            .address
            .details
            .as_ref()
            .ok_or(SessionError::InvalidSelection)?;
        if details
            .selection
            .as_ref()
            .is_some_and(|choice| choice.operator.id != verified.operator.id)
            || verified.binding.provider_id != verified.catalog.provider_id
            || verified.binding.source_id.is_empty()
            || verified.catalog.street_id.is_empty()
            || verified.catalog.house_id.is_empty()
            || verified
                .queue
                .as_ref()
                .is_some_and(|queue| queue.operator_id != verified.operator.id)
        {
            return Err(SessionError::InvalidSelection);
        }
        let saved = self
            .state
            .addresses
            .iter_mut()
            .find(|entry| entry.id == id)
            .ok_or(SessionError::AddressNotFound)?;
        let details = saved
            .address
            .details
            .as_mut()
            .ok_or(SessionError::InvalidSelection)?;
        saved.address.queue = verified
            .queue
            .as_ref()
            .map(|queue| queue.code.clone())
            .or_else(|| {
                details
                    .selection
                    .as_ref()
                    .and_then(|choice| choice.queue.as_ref())
                    .map(|queue| queue.code.clone())
            });
        details.operator = Some(verified.operator);
        details.binding = Some(verified.binding);
        details.catalog = Some(verified.catalog);
        details.queue = verified.queue;
        Ok(self.state())
    }
    pub fn set_address_options(
        &mut self,
        id: &str,
        monitoring: bool,
        notifications: bool,
    ) -> Result<SessionState, SessionError> {
        let entry = self
            .state
            .addresses
            .iter_mut()
            .find(|entry| entry.id == id)
            .ok_or(SessionError::AddressNotFound)?;
        entry.monitoring_enabled = monitoring;
        entry.notifications_enabled = notifications;
        Ok(self.state())
    }
    /// Validate all monitoring fields before changing any session preference.
    pub fn set_monitoring(
        &mut self,
        interval: u16,
        monitoring: MonitoringSettings,
    ) -> Result<SessionState, SessionError> {
        if ![15, 30, 60].contains(&interval) || !monitoring.is_valid() {
            return Err(SessionError::InvalidMonitoring);
        }
        self.state.check_interval_minutes = interval;
        self.state.monitoring = monitoring;
        Ok(self.state())
    }
    pub fn state(&self) -> SessionState {
        self.state.clone()
    }
    pub fn get(&self, id: &str) -> Result<&SavedAddress, SessionError> {
        self.state
            .addresses
            .iter()
            .find(|entry| entry.id == id)
            .ok_or(SessionError::AddressNotFound)
    }
    /// Store user text without inventing a source binding or queue. Territory
    /// metadata is supplied only after the application validates catalog IDs.
    pub fn save_manual(
        &mut self,
        input: AddressInput,
        territory: Option<(Oblast, Option<String>)>,
        label: &str,
        editing_id: Option<&str>,
        selection: Option<OperatorSelection>,
    ) -> Result<SessionState, SessionError> {
        let mut details =
            AddressDetails::unresolved(input).map_err(|_| SessionError::InvalidSelection)?;
        if selection.as_ref().is_some_and(|value| {
            value.operator.id.is_empty()
                || value.queue.as_ref().is_some_and(|queue| {
                    queue.operator_id != value.operator.id || queue.code.is_empty()
                })
        }) {
            return Err(SessionError::InvalidSelection);
        }
        details.selection = selection;
        if let Some((oblast, settlement_id)) = territory {
            details.oblast = Some(oblast);
            details.settlement_katottg = settlement_id;
        }
        if self.state.addresses.iter().any(|entry| {
            Some(entry.id.as_str()) != editing_id
                && entry.address.details.as_ref().is_some_and(|value| {
                    value.normalized == details.normalized
                        && value.settlement_katottg == details.settlement_katottg
                        && value.selection.as_ref().map(|s| &s.operator.id)
                            == details.selection.as_ref().map(|s| &s.operator.id)
                })
        }) {
            return Err(SessionError::DuplicateAddress);
        }
        let previous = editing_id.map(|id| self.get(id)).transpose()?;
        let unchanged = previous
            .and_then(|entry| entry.address.details.as_ref())
            .is_some_and(|value| {
                value.normalized == details.normalized
                    && value.settlement_katottg == details.settlement_katottg
                    && value.selection == details.selection
            });
        let source_id = if unchanged {
            previous
                .expect("unchanged address exists")
                .address
                .id
                .clone()
        } else {
            format!("manual-{}", self.next_id)
        };
        let next_id = self
            .next_id
            .checked_add(1)
            .ok_or(SessionError::InvalidSelection)?;
        let source = Address {
            id: source_id,
            provider_id: "manual".into(),
            settlement_id: details.settlement_katottg.clone().unwrap_or_default(),
            street_id: String::new(),
            settlement: details.original.settlement.trim().into(),
            street: details.original.street.trim().into(),
            house: details.original.house.trim().into(),
            label: String::new(),
            queue: details
                .selection
                .as_ref()
                .and_then(|s| s.queue.as_ref())
                .map(|q| q.code.clone()),
            details: Some(details),
        };
        let group = source.queue.clone();
        let state = self.save(source, group.as_deref(), label, editing_id)?;
        if editing_id.is_some() && !unchanged {
            self.next_id = next_id;
        }
        Ok(state)
    }
    /// Caller has validated this geographic reference against the immutable
    /// catalog. It never upgrades the operator binding or confirms a queue.
    pub fn set_geographic(
        &mut self,
        id: &str,
        geographic: crate::grid::GeographicAddressSelection,
    ) -> Result<SessionState, SessionError> {
        let saved = self
            .state
            .addresses
            .iter_mut()
            .find(|a| a.id == id)
            .ok_or(SessionError::AddressNotFound)?;
        let details = saved
            .address
            .details
            .as_mut()
            .ok_or(SessionError::InvalidSelection)?;
        if details.settlement_katottg.as_deref() != Some(&geographic.settlement_id) {
            return Err(SessionError::InvalidSelection);
        }
        details.geographic = Some(geographic);
        Ok(self.state())
    }
    /// Callers pass the canonical catalog record, never fields supplied by the UI.
    /// A group must match the source; None remains a legitimate unknown group.
    pub fn save(
        &mut self,
        mut source: Address,
        group: Option<&str>,
        label: &str,
        editing_id: Option<&str>,
    ) -> Result<SessionState, SessionError> {
        if source.queue.as_deref() != group {
            return Err(SessionError::InvalidSelection);
        }
        let label = label.trim();
        if label.is_empty() || label.chars().count() > 40 {
            return Err(SessionError::InvalidName);
        }
        if let Some(id) = editing_id {
            self.get(id)?;
        }
        if self.state.addresses.iter().any(|entry| {
            Some(entry.id.as_str()) != editing_id
                && entry.address.provider_id == source.provider_id
                && entry.address.id == source.id
        }) {
            return Err(SessionError::DuplicateAddress);
        }
        source.label = label.into();
        let id = match editing_id {
            Some(id) => id.to_owned(),
            None => {
                let id = format!("local-{}", self.next_id);
                self.next_id = self
                    .next_id
                    .checked_add(1)
                    .ok_or(SessionError::InvalidSelection)?;
                id
            }
        };
        let saved = SavedAddress {
            id: id.clone(),
            address: source,
            monitoring_enabled: self
                .state
                .addresses
                .iter()
                .find(|entry| entry.id == id)
                .is_none_or(|entry| entry.monitoring_enabled),
            notifications_enabled: self
                .state
                .addresses
                .iter()
                .find(|entry| entry.id == id)
                .is_none_or(|entry| entry.notifications_enabled),
        };
        if let Some(entry) = self.state.addresses.iter_mut().find(|entry| entry.id == id) {
            *entry = saved;
        } else {
            self.state.addresses.push(saved);
        }
        self.state.active_address_id = Some(id);
        Ok(self.state())
    }
    pub fn activate(&mut self, id: &str) -> Result<SessionState, SessionError> {
        self.get(id)?;
        self.state.active_address_id = Some(id.into());
        Ok(self.state())
    }
    /// Pick a remaining address deterministically; deleting the last one returns
    /// to the first-run flow rather than leaving a deleted address on the screen.
    pub fn remove(&mut self, id: &str) -> Result<SessionState, SessionError> {
        self.get(id)?;
        self.state.addresses.retain(|entry| entry.id != id);
        if self.state.active_address_id.as_deref() == Some(id) {
            self.state.active_address_id =
                self.state.addresses.first().map(|entry| entry.id.clone());
        }
        if self.state.addresses.is_empty() {
            self.state.onboarding_complete = false;
        }
        Ok(self.state())
    }
    pub fn complete_onboarding(&mut self, interval: u16) -> Result<SessionState, SessionError> {
        if self.state.addresses.is_empty() || ![15, 30, 60].contains(&interval) {
            return Err(SessionError::InvalidPreferences);
        }
        self.state.check_interval_minutes = interval;
        self.state.onboarding_complete = true;
        Ok(self.state())
    }
}
