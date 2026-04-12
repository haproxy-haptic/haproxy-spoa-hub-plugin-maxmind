use std::collections::BTreeMap;
use std::net::IpAddr;

use maxminddb::{MaxMindDbError, Reader};

use crate::config::{DatabaseEntry, Lookup, PluginConfig};
use crate::path::{AnyLeaf, compile_path, path_display};
use crate::warn_log::WarnOnce;

/// All MMDB readers loaded into memory at plugin init, keyed by operator alias.
pub struct LoadedDatabases {
    pub readers: BTreeMap<String, Reader<Vec<u8>>>,
}

impl LoadedDatabases {
    /// Open every configured database file exactly once.
    pub fn open(entries: &BTreeMap<String, DatabaseEntry>) -> Result<Self, String> {
        let mut readers = BTreeMap::new();
        for (alias, entry) in entries {
            let reader = Reader::open_readfile(&entry.path).map_err(|e| {
                format!("failed to open database '{alias}' at '{}': {e}", entry.path)
            })?;
            readers.insert(alias.clone(), reader);
        }
        Ok(Self { readers })
    }
}

/// A `Lookup` augmented with hot-path-friendly precomputed fields.
#[derive(Debug)]
pub struct CompiledLookup {
    pub name: String,
    pub database: String,
    pub input: String,
    pub path: Vec<String>,
    pub path_display: String,
    pub output_var: String,
    pub status_var: String,
}

impl CompiledLookup {
    fn from_lookup(lookup: Lookup) -> Self {
        let path_display = path_display(&lookup.path);
        let status_var = format!("{}_status", lookup.output_var);
        Self {
            name: lookup.name,
            database: lookup.database,
            input: lookup.input,
            path: lookup.path,
            path_display,
            output_var: lookup.output_var,
            status_var,
        }
    }
}

/// Routes incoming SPOE messages to the lookups bound to them. One message
/// name may map to multiple lookups (clarification Q2); the plugin executes all
/// of them and returns the union of their output variables.
pub struct LookupRouter {
    by_message: BTreeMap<String, Vec<usize>>,
    pub compiled: Vec<CompiledLookup>,
}

impl LookupRouter {
    pub fn from_lookups(lookups: Vec<Lookup>) -> Self {
        let mut by_message: BTreeMap<String, Vec<usize>> = BTreeMap::new();
        let mut compiled: Vec<CompiledLookup> = Vec::with_capacity(lookups.len());
        for (idx, lookup) in lookups.into_iter().enumerate() {
            by_message
                .entry(lookup.message.clone())
                .or_default()
                .push(idx);
            compiled.push(CompiledLookup::from_lookup(lookup));
        }
        Self {
            by_message,
            compiled,
        }
    }

    pub fn lookups_for(&self, message_name: &str) -> Option<&[usize]> {
        self.by_message.get(message_name).map(Vec::as_slice)
    }

    pub fn get(&self, idx: usize) -> &CompiledLookup {
        &self.compiled[idx]
    }
}

/// Outcome of executing a single lookup. Maps directly to the
/// `<output>_status` sibling variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookupStatus {
    Ok,
    NotFound,
    Error,
}

impl LookupStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::NotFound => "not_found",
            Self::Error => "error",
        }
    }
}

/// Execute one compiled lookup against an IP and return the (value, status)
/// pair to emit as transaction variables.
///
/// On `LookupStatus::Ok` the value is the rendered leaf; otherwise the value
/// is the empty string. The status enum tells the caller whether to emit a
/// warning (NotFound for a missing field path, Error for a typo / non-leaf).
pub fn execute_lookup(
    lookup: &CompiledLookup,
    databases: &LoadedDatabases,
    ip: IpAddr,
    warn: &WarnOnce,
) -> (String, LookupStatus) {
    let reader = match databases.readers.get(&lookup.database) {
        Some(r) => r,
        None => {
            // Validation guarantees this is unreachable, but bail safely if it
            // ever isn't — surfacing as Error rather than panicking.
            log::error!(
                "lookup '{}' references unknown database alias '{}' at runtime",
                lookup.name,
                lookup.database
            );
            return (String::new(), LookupStatus::Error);
        }
    };

    let result = match reader.lookup(ip) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("lookup '{}' failed for {ip}: {e}", lookup.name);
            return (String::new(), LookupStatus::Error);
        }
    };

    if !result.has_data() {
        // IP not in database — normal traffic, no warn-once entry.
        return (String::new(), LookupStatus::NotFound);
    }

    let elements = compile_path(&lookup.path);
    match result.decode_path::<AnyLeaf>(&elements) {
        Ok(Some(leaf)) => (leaf.into_string(), LookupStatus::Ok),
        Ok(None) => {
            if warn.should_warn(&lookup.name, &lookup.path_display) {
                log::warn!(
                    "lookup '{}': MMDB field path '{}' missing for {ip}",
                    lookup.name,
                    lookup.path_display
                );
            }
            (String::new(), LookupStatus::NotFound)
        }
        Err(MaxMindDbError::Decoding { message, .. }) => {
            if warn.should_warn(&lookup.name, &lookup.path_display) {
                log::warn!(
                    "lookup '{}': MMDB path '{}' did not address a leaf value: {message}",
                    lookup.name,
                    lookup.path_display
                );
            }
            (String::new(), LookupStatus::Error)
        }
        Err(e) => {
            if warn.should_warn(&lookup.name, &lookup.path_display) {
                log::warn!(
                    "lookup '{}': MMDB error for {ip} at '{}': {e}",
                    lookup.name,
                    lookup.path_display
                );
            }
            (String::new(), LookupStatus::Error)
        }
    }
}

/// Build a `LookupRouter` and `LoadedDatabases` from a parsed `PluginConfig`.
/// Convenience wrapper used by the plugin entry point.
pub fn load(config: PluginConfig) -> Result<(LoadedDatabases, LookupRouter), String> {
    let databases = LoadedDatabases::open(&config.databases)?;
    let router = LookupRouter::from_lookups(config.lookups);

    // Log unreferenced database aliases so operators notice unused loads.
    for alias in databases.readers.keys() {
        if !router.compiled.iter().any(|l| l.database == *alias) {
            log::info!("database '{alias}' is configured but not referenced by any lookup");
        }
    }

    Ok((databases, router))
}
