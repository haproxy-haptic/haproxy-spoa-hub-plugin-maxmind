use haproxy_spoa_hub_plugin_api::PluginContext;

use crate::db_type::DatabaseType;

/// A single database entry from configuration.
#[derive(Debug)]
pub struct DatabaseEntry {
    pub db_type: DatabaseType,
    pub path: String,
}

/// Plugin configuration parsed from hub's `[plugins.params]` TOML table.
#[derive(Debug)]
pub struct PluginConfig {
    pub databases: Vec<DatabaseEntry>,
    pub locale: String,
    pub fields: Option<Vec<String>>,
}

impl Default for PluginConfig {
    fn default() -> Self {
        PluginConfig {
            databases: Vec::new(),
            locale: "en".into(),
            fields: None,
        }
    }
}

impl PluginConfig {
    pub fn from_context(context: &PluginContext) -> Result<Self, String> {
        let locale = context
            .config
            .get("locale")
            .and_then(|v| v.as_str())
            .unwrap_or("en")
            .to_string();

        let fields = context
            .config
            .get("fields")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            });

        let databases = context
            .config
            .get("databases")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "missing required 'databases' configuration array".to_string())?;

        let mut db_entries = Vec::with_capacity(databases.len());
        for entry in databases.iter() {
            let table = entry.as_table().ok_or_else(|| {
                "each database entry must be a table with 'type' and 'path'".to_string()
            })?;

            let type_str = table
                .get("type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "database entry missing 'type' field".to_string())?;

            let db_type = DatabaseType::parse(type_str).ok_or_else(|| {
                format!(
                    "unknown database type '{type_str}', expected one of: city, country, asn, anonymous, isp, conntype, domain"
                )
            })?;

            let path = table
                .get("path")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    format!("database entry for type '{type_str}' missing 'path' field")
                })?
                .to_string();

            db_entries.push(DatabaseEntry { db_type, path });
        }

        if db_entries.is_empty() {
            return Err("'databases' array must contain at least one entry".into());
        }

        Ok(PluginConfig {
            databases: db_entries,
            locale,
            fields,
        })
    }

    pub fn config_schema() -> &'static str {
        r#"{
            "type": "object",
            "required": ["databases"],
            "properties": {
                "locale": { "type": "string", "default": "en" },
                "fields": {
                    "type": "array",
                    "items": { "type": "string" }
                },
                "databases": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "required": ["type", "path"],
                        "properties": {
                            "type": {
                                "type": "string",
                                "enum": ["city", "country", "asn", "anonymous", "isp", "conntype", "domain"]
                            },
                            "path": { "type": "string" }
                        },
                        "additionalProperties": false
                    }
                }
            },
            "additionalProperties": false
        }"#
    }
}
