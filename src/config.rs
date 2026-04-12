use std::collections::BTreeMap;

use abi_stable::std_types::Tuple2;
use haproxy_spoa_hub_plugin_api::{ConfigValue, PluginContext};

const DEFAULT_INPUT_ARG: &str = "input";

/// Operator-defined database alias bound to a single MMDB file.
#[derive(Debug, Clone)]
pub struct DatabaseEntry {
    pub path: String,
}

/// Operator-defined named extraction. Each lookup binds one SPOE message
/// argument to one field path inside one database, producing one transaction
/// variable plus a sibling status variable.
#[derive(Debug, Clone)]
pub struct Lookup {
    pub name: String,
    pub message: String,
    pub database: String,
    pub input: String,
    pub path: Vec<String>,
    pub output_var: String,
}

/// Plugin configuration parsed from the hub's `[plugins.params]` table.
#[derive(Debug, Clone)]
pub struct PluginConfig {
    pub databases: BTreeMap<String, DatabaseEntry>,
    pub lookups: Vec<Lookup>,
}

impl PluginConfig {
    pub fn from_context(context: &PluginContext) -> Result<Self, String> {
        // Reject any 0.1.0 keys with a clear migration pointer before doing
        // anything else, so operators upgrading get a useful error.
        for legacy in ["locale", "fields"] {
            if context.config.get(legacy).is_some() {
                return Err(format!(
                    "configuration key '{legacy}' is no longer supported in 0.2.0; \
                     see the 'Migration from 0.1.0' section of the README for the new \
                     alias-and-lookup configuration shape"
                ));
            }
        }

        let databases = parse_databases(&context.config)?;
        let lookups = parse_lookups(&context.config)?;

        let config = PluginConfig { databases, lookups };
        config.validate()?;
        Ok(config)
    }

    /// Enforce the structural rules from `data-model.md`. Called automatically
    /// from `from_context`; exposed for tests that build a `PluginConfig`
    /// directly.
    pub fn validate(&self) -> Result<(), String> {
        if self.databases.is_empty() {
            return Err("'databases' must contain at least one entry".into());
        }
        if self.lookups.is_empty() {
            return Err("'lookups' must contain at least one entry".into());
        }

        let mut seen_names: BTreeMap<&str, ()> = BTreeMap::new();
        let mut seen_outputs: BTreeMap<&str, &str> = BTreeMap::new();

        for lookup in &self.lookups {
            if lookup.name.is_empty() {
                return Err("lookup with empty 'name' is not allowed".into());
            }
            if lookup.message.is_empty() {
                return Err(format!("lookup '{}' has empty 'message'", lookup.name));
            }
            if lookup.database.is_empty() {
                return Err(format!("lookup '{}' has empty 'database'", lookup.name));
            }
            if lookup.path.is_empty() {
                return Err(format!("lookup '{}' has empty 'path'", lookup.name));
            }
            if lookup.output_var.is_empty() {
                return Err(format!("lookup '{}' has empty 'output_var'", lookup.name));
            }
            if !self.databases.contains_key(&lookup.database) {
                return Err(format!(
                    "unknown database alias '{}' referenced by lookup '{}'",
                    lookup.database, lookup.name
                ));
            }
            if seen_names.insert(lookup.name.as_str(), ()).is_some() {
                return Err(format!("duplicate lookup name '{}'", lookup.name));
            }
            if let Some(prev) =
                seen_outputs.insert(lookup.output_var.as_str(), lookup.name.as_str())
            {
                return Err(format!(
                    "duplicate output variable '{}' (used by lookups '{}' and '{}')",
                    lookup.output_var, prev, lookup.name
                ));
            }
        }

        Ok(())
    }

    pub fn config_schema() -> &'static str {
        r#"{
            "type": "object",
            "required": ["databases", "lookups"],
            "properties": {
                "databases": {
                    "type": "object",
                    "minProperties": 1,
                    "additionalProperties": {
                        "type": "object",
                        "required": ["path"],
                        "properties": {
                            "path": { "type": "string" }
                        }
                    }
                },
                "lookups": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "required": ["name", "message", "database", "path", "output_var"],
                        "properties": {
                            "name":       { "type": "string", "minLength": 1 },
                            "message":    { "type": "string", "minLength": 1 },
                            "database":   { "type": "string", "minLength": 1 },
                            "input":      { "type": "string", "minLength": 1 },
                            "path":       {
                                "type": "array",
                                "minItems": 1,
                                "items": { "type": "string", "minLength": 1 }
                            },
                            "output_var": { "type": "string", "minLength": 1 }
                        }
                    }
                }
            }
        }"#
    }
}

fn parse_databases(
    config: &abi_stable::std_types::RHashMap<abi_stable::std_types::RString, ConfigValue>,
) -> Result<BTreeMap<String, DatabaseEntry>, String> {
    let value = config
        .get("databases")
        .ok_or_else(|| "missing required 'databases' configuration table".to_string())?;
    let table = value
        .as_table()
        .ok_or_else(|| "'databases' must be a table mapping alias → entry".to_string())?;

    let mut out = BTreeMap::new();
    for Tuple2(alias, entry_value) in table.iter() {
        let entry_table = entry_value
            .as_table()
            .ok_or_else(|| format!("database '{alias}' must be a table with a 'path' field"))?;

        // Reject 0.1.0 type-based databases (an array of {type, path} entries).
        if entry_table.get("type").is_some() {
            return Err(format!(
                "database '{alias}' uses the 0.1.0 'type' field which is no longer supported; \
                 see the 'Migration from 0.1.0' section of the README"
            ));
        }

        let path = entry_table
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("database '{alias}' missing required 'path' string"))?
            .to_string();
        out.insert(alias.as_str().to_string(), DatabaseEntry { path });
    }

    // Also catch the 0.1.0 array shape (`[[plugins.params.databases]]`) which
    // serializes as a ConfigValue::Array, not Table.
    if matches!(value, ConfigValue::Array(_)) {
        return Err(
            "'databases' must be a table mapping alias → entry; the 0.1.0 array form is no \
             longer supported. See the 'Migration from 0.1.0' section of the README"
                .to_string(),
        );
    }

    Ok(out)
}

fn parse_lookups(
    config: &abi_stable::std_types::RHashMap<abi_stable::std_types::RString, ConfigValue>,
) -> Result<Vec<Lookup>, String> {
    let value = config
        .get("lookups")
        .ok_or_else(|| "missing required 'lookups' configuration array".to_string())?;
    let array = value
        .as_array()
        .ok_or_else(|| "'lookups' must be an array of lookup tables".to_string())?;

    let mut out = Vec::with_capacity(array.len());
    for (i, entry) in array.iter().enumerate() {
        let table = entry.as_table().ok_or_else(|| {
            format!("lookup #{i} must be a table with name/message/database/path/output_var")
        })?;

        let name = required_string(table, "name", &format!("lookup #{i}"))?;
        let context = format!("lookup '{name}'");
        let message = required_string(table, "message", &context)?;
        let database = required_string(table, "database", &context)?;
        let output_var = required_string(table, "output_var", &context)?;
        let input = table
            .get("input")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| DEFAULT_INPUT_ARG.to_string());

        let path_value = table
            .get("path")
            .ok_or_else(|| format!("{context} missing required 'path' array"))?;
        let path_array = path_value
            .as_array()
            .ok_or_else(|| format!("{context} field 'path' must be an array of strings"))?;
        let path: Result<Vec<String>, String> = path_array
            .iter()
            .enumerate()
            .map(|(j, v)| {
                v.as_str()
                    .map(|s| s.to_string())
                    .ok_or_else(|| format!("{context} field 'path[{j}]' must be a string"))
            })
            .collect();
        let path = path?;

        out.push(Lookup {
            name,
            message,
            database,
            input,
            path,
            output_var,
        });
    }

    Ok(out)
}

fn required_string(
    table: &abi_stable::std_types::RHashMap<abi_stable::std_types::RString, ConfigValue>,
    key: &str,
    context: &str,
) -> Result<String, String> {
    table
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{context} missing required '{key}' string"))
}
