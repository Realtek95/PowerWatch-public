//! Immutable prepared country catalog. No XML/PBF processing or operator
//! inference happens here; a geographic address never proves an outage queue.
mod install;
pub use install::{CatalogCompression, CatalogManifest, install};
use powerwatch_core::model::CatalogEntry;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use unicode_normalization::UnicodeNormalization;
#[derive(Debug, thiserror::Error)]
pub enum CatalogError {
    #[error("invalid catalog selection")]
    InvalidSelection,
    #[error("invalid catalog package")]
    InvalidPackage,
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
type Result<T> = std::result::Result<T, CatalogError>;
pub fn normalize(value: &str) -> String {
    value
        .nfkc()
        .flat_map(char::to_lowercase)
        .map(|c| {
            if matches!(c, '’' | 'ʼ' | '`') {
                '\''
            } else {
                c
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HouseEntry {
    pub id: String,
    pub name: String,
    pub number: String,
    pub building: String,
    pub postcode: Option<String>,
}
pub use powerwatch_core::grid::GeographicAddressSelection as GeographicSelection;
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordSource {
    pub source: String,
    pub native_id: Option<String>,
    pub updated_at: Option<String>,
    pub retrieved_at: Option<String>,
}
pub struct Catalog {
    connection: Connection,
    version: String,
}
impl Catalog {
    pub fn open(path: &Path, version: &str) -> Result<Self> {
        if version.len() != 64 || !version.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(CatalogError::InvalidPackage);
        }
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.execute_batch("PRAGMA query_only=ON; PRAGMA cache_size=-4096;")?;
        if connection.pragma_query_value::<i32, _>(None, "application_id", |r| r.get(0))?
            != 1_347_895_620
            || connection.pragma_query_value::<i32, _>(None, "user_version", |r| r.get(0))? != 1
        {
            return Err(CatalogError::InvalidPackage);
        }
        Ok(Self {
            connection,
            version: version.into(),
        })
    }
    fn entry_id(&self, kind: &str, id: i64) -> String {
        format!("geo:{}:{kind}:{id}", self.version)
    }
    fn row_id(&self, id: &str, kind: &str) -> Result<i64> {
        id.strip_prefix(&format!("geo:{}:{kind}:", self.version))
            .and_then(|id| id.parse().ok())
            .filter(|id: &i64| *id > 0)
            .ok_or(CatalogError::InvalidSelection)
    }
    pub fn version(&self) -> &str {
        &self.version
    }
    pub fn districts(&self, oblast: &str) -> Result<Vec<CatalogEntry>> {
        self.entries(
            "SELECT id,name FROM districts WHERE oblast_id=?1 ORDER BY norm,id",
            [oblast],
        )
    }
    pub fn communities(&self, oblast: &str, district: Option<&str>) -> Result<Vec<CatalogEntry>> {
        self.entries(
            "SELECT c.id,c.name||CASE WHEN ?2 IS NULL AND d.name IS NOT NULL THEN ' · '||d.name||' район' ELSE '' END FROM communities c LEFT JOIN districts d ON d.id=c.district_id WHERE c.oblast_id=?1 AND (?2 IS NULL OR c.district_id=?2) ORDER BY c.norm,c.id",
            params![oblast, district],
        )
    }
    pub fn settlements(
        &self,
        oblast: &str,
        district: Option<&str>,
        community: Option<&str>,
    ) -> Result<Vec<CatalogEntry>> {
        self.entries(
            "SELECT s.id,s.name||CASE WHEN c.name IS NOT NULL THEN ' · '||c.name||' громада' ELSE '' END||CASE WHEN d.name IS NOT NULL THEN ', '||d.name||' район' ELSE '' END FROM settlements s LEFT JOIN districts d ON d.id=s.district_id LEFT JOIN communities c ON c.id=s.community_id WHERE s.oblast_id=?1 AND (?2 IS NULL OR s.district_id=?2) AND (?3 IS NULL OR s.community_id=?3) ORDER BY s.norm,s.id",
            params![oblast, district, community],
        )
    }
    fn entries(&self, sql: &str, values: impl rusqlite::Params) -> Result<Vec<CatalogEntry>> {
        Ok(self
            .connection
            .prepare(sql)?
            .query_map(values, |r| {
                Ok(CatalogEntry {
                    id: r.get(0)?,
                    name: r.get(1)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?)
    }
    pub fn streets(&self, settlement: &str, query: &str) -> Result<Vec<CatalogEntry>> {
        let query = normalize(query);
        if query.chars().count() < 2 || query.len() > 800 {
            return Ok(Vec::new());
        }
        let tokens: Vec<String> = query
            .split_whitespace()
            .take(8)
            .map(|word| {
                let word = word.trim_matches(|c: char| !c.is_alphanumeric());
                match word {
                    "вул" => "вулиця",
                    "пров" => "провулок",
                    "пр-т" | "просп" => "проспект",
                    "пл" => "площа",
                    "бул" | "бульв" => "бульвар",
                    "наб" => "набережна",
                    "узв" => "узвіз",
                    _ => word,
                }
                .to_owned()
            })
            .filter(|word| !word.is_empty())
            .collect();
        if tokens.is_empty() {
            return Ok(Vec::new());
        }
        let clauses = (0..tokens.len())
            .map(|i| format!("instr(n.norm,?{})>0", i + 2))
            .collect::<Vec<_>>()
            .join(" AND ");
        let sql = format!(
            "SELECT s.id,s.name FROM streets s WHERE s.settlement_id=?1 AND EXISTS(SELECT 1 FROM street_names n WHERE n.street_id=s.id AND n.settlement_id=?1 AND {clauses}) ORDER BY s.rank DESC,s.norm,s.id LIMIT 40"
        );
        let mut values = vec![settlement.to_owned()];
        values.extend(tokens);
        let mut statement = self.connection.prepare(&sql)?;
        Ok(statement
            .query_map(rusqlite::params_from_iter(values), |r| {
                Ok(CatalogEntry {
                    id: self.entry_id("street", r.get(0)?),
                    name: r.get(1)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?)
    }
    pub fn houses(&self, settlement: &str, street: &str, query: &str) -> Result<Vec<HouseEntry>> {
        let street = self.row_id(street, "street")?;
        let query = normalize(query);
        if query.len() > 200 {
            return Ok(Vec::new());
        }
        let mut statement = self
            .connection
            .prepare(
                "SELECT h.id,h.number,h.building,h.postcode FROM houses h JOIN streets s ON s.id=h.street_id WHERE s.settlement_id=?1 AND s.id=?2 AND substr(h.number_norm,1,length(?3))=?3 ORDER BY length(h.number_norm),h.number_norm,h.building_norm,h.id LIMIT 40",
            )?;
        Ok(statement
            .query_map(params![settlement, street, query], |r| {
                let number: String = r.get(1)?;
                let building: String = r.get(2)?;
                Ok(HouseEntry {
                    id: self.entry_id("house", r.get(0)?),
                    name: if building.is_empty() {
                        number.clone()
                    } else {
                        format!("{number}, корп. {building}")
                    },
                    number,
                    building,
                    postcode: r.get(3)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?)
    }
    /// Validate every parent and selected display value again at the IPC edge.
    /// This selection has no relationship to an operator's address identifiers.
    pub fn validate(
        &self,
        oblast: &str,
        selection: &GeographicSelection,
        input: &powerwatch_core::grid::AddressInput,
    ) -> Result<()> {
        if selection.version != self.version {
            return Err(CatalogError::InvalidSelection);
        }
        let locality: Option<(String, Option<String>, Option<String>)> = self
            .connection
            .query_row(
                "SELECT name,district_id,community_id FROM settlements WHERE id=?1 AND oblast_id=?2",
                params![selection.settlement_id, oblast],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((name, district, community)) = locality else {
            return Err(CatalogError::InvalidSelection);
        };
        if name != input.settlement.trim()
            || selection
                .district_id
                .as_ref()
                .is_some_and(|v| Some(v) != district.as_ref())
            || selection
                .community_id
                .as_ref()
                .is_some_and(|v| Some(v) != community.as_ref())
        {
            return Err(CatalogError::InvalidSelection);
        }
        if let Some(street_id) = &selection.street_id {
            let street = self.row_id(street_id, "street")?;
            let name: Option<String> = self
                .connection
                .query_row(
                    "SELECT name FROM streets WHERE id=?1 AND settlement_id=?2",
                    params![street, selection.settlement_id],
                    |r| r.get(0),
                )
                .optional()?;
            if name.as_deref() != Some(input.street.trim()) {
                return Err(CatalogError::InvalidSelection);
            }
            if let Some(house_id) = &selection.house_id {
                let house = self.row_id(house_id, "house")?;
                let value: Option<(String, String, Option<String>)> = self
                    .connection
                    .query_row(
                        "SELECT number,building,postcode FROM houses WHERE id=?1 AND street_id=?2",
                        params![house, street],
                        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                    )
                    .optional()?;
                if value.as_ref().is_none_or(|(n, b, p)| {
                    n != input.house.trim()
                        || b != input.building.trim()
                        || p != &selection.postcode
                }) {
                    return Err(CatalogError::InvalidSelection);
                }
            } else if selection.postcode.is_some() {
                return Err(CatalogError::InvalidSelection);
            }
        } else if selection.house_id.is_some() || selection.postcode.is_some() {
            return Err(CatalogError::InvalidSelection);
        }
        Ok(())
    }
    pub fn sources(&self, id: &str, kind: &str) -> Result<Vec<RecordSource>> {
        if !matches!(kind, "street" | "house") {
            return Err(CatalogError::InvalidSelection);
        }
        let id = self.row_id(id, kind)?;
        Ok(
            self
                .connection
                .prepare(
                    "SELECT r.source,r.native_id,coalesce(r.updated_at,s.updated_at),s.retrieved_at FROM source_records r LEFT JOIN source_snapshots s ON s.id=r.snapshot_id WHERE r.kind=?1 AND r.entity_id=?2 ORDER BY r.source,r.native_id LIMIT 100",
                )?
                .query_map(
                    params![kind, id],
                    |r| Ok(RecordSource {
                        source: r.get(0)?,
                        native_id: r.get(1)?,
                        updated_at: r.get(2)?,
                        retrieved_at: r.get(3)?,
                    }),
                )?
                .collect::<std::result::Result<_, _>>()?,
        )
    }
}
