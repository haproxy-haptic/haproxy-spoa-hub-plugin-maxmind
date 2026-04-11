use std::net::IpAddr;

use maxminddb::Reader;

use crate::config::DatabaseEntry;
use crate::db_type::DatabaseType;

/// A loaded MMDB database ready for lookups.
pub struct LoadedDatabase {
    pub db_type: DatabaseType,
    pub reader: Reader<Vec<u8>>,
}

/// Load all configured databases into memory.
pub fn load_databases(
    entries: &[DatabaseEntry],
) -> Result<Vec<LoadedDatabase>, Box<dyn std::error::Error + Send + Sync>> {
    let mut databases = Vec::with_capacity(entries.len());
    for entry in entries {
        let reader = Reader::open_readfile(&entry.path).map_err(|e| {
            format!(
                "failed to open {:?} database at '{}': {e}",
                entry.db_type, entry.path
            )
        })?;
        databases.push(LoadedDatabase {
            db_type: entry.db_type,
            reader,
        });
    }
    Ok(databases)
}

/// Look up an IP address in all loaded databases and return field pairs.
pub fn lookup_ip(
    ip: IpAddr,
    databases: &[LoadedDatabase],
    locale: &str,
    fields: Option<&[String]>,
) -> Vec<(String, String)> {
    let mut results = Vec::new();
    for db in databases {
        let extracted = db.db_type.extract_fields(&db.reader, ip, locale);
        results.extend(extracted);
    }

    if let Some(allowed) = fields {
        results.retain(|(name, _)| allowed.iter().any(|f| f == name));
    }

    results
}
