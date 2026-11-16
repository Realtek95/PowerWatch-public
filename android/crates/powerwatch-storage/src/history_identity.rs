//! Address revisions may change source IDs/queue choices without moving house.
//! Group only the same saved local address and an unchanged physical identity.
use crate::{Result, Store, decode};
use powerwatch_core::model::Address;
use rusqlite::OptionalExtension;
fn same_place(left: &Address, right: &Address) -> bool {
    match (&left.details, &right.details) {
        (Some(left), Some(right)) => {
            if left.normalized != right.normalized
                || left.settlement_katottg != right.settlement_katottg
                || left.oblast.as_ref().map(|o| &o.id) != right.oblast.as_ref().map(|o| &o.id)
            {
                return false;
            }
            if let (Some(a), Some(b)) = (&left.geographic, &right.geographic) {
                let compatible = |a: &Option<String>, b: &Option<String>| {
                    a.as_ref().zip(b.as_ref()).is_none_or(|(a, b)| a == b)
                };
                if a.settlement_id != b.settlement_id
                    || !compatible(&a.district_id, &b.district_id)
                    || !compatible(&a.community_id, &b.community_id)
                {
                    return false;
                }
                if a.version == b.version
                    && (!compatible(&a.street_id, &b.street_id)
                        || !compatible(&a.house_id, &b.house_id))
                {
                    return false;
                }
            }
            true
        }
        (None, None) => {
            left.provider_id == right.provider_id
                && !left.settlement_id.is_empty()
                && !left.street_id.is_empty()
                && left.settlement_id == right.settlement_id
                && left.street_id == right.street_id
                && left.house == right.house
        }
        _ => false,
    }
}
impl Store {
    pub(crate) fn history_keys(&self, key: &str) -> Result<Vec<String>> {
        let selected: Option<(String, String)> = self
            .conn
            .query_row(
                "SELECT local_id,address_json FROM addresses WHERE key=?1",
                [key],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((id, json)) = selected else {
            return Ok(vec![key.into()]);
        };
        let address: Address = decode(&json)?;
        let mut result = vec![key.to_owned()];
        let mut statement = self.conn.prepare(
            "SELECT key,address_json FROM addresses WHERE local_id=?1 AND key!=?2 ORDER BY key",
        )?;
        let rows = statement.query_map([&id, key], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (other, json) = row?;
            if same_place(&address, &decode(&json)?) {
                result.push(other);
            }
        }
        Ok(result)
    }
    pub(crate) fn current_history_key(&self, key: &str) -> Result<Option<String>> {
        Ok(
            self
                .conn
                .query_row(
                    "SELECT key FROM addresses WHERE retired=0 AND local_id=(SELECT local_id FROM addresses WHERE key=?1)",
                    [key],
                    |row| row.get(0),
                )
                .optional()?,
        )
    }
}
