use std::net::Ipv4Addr;
use std::path::Path;

use abi_stable::std_types::{RHashMap, RString, RVec};
use haproxy_spoa_hub_plugin_api::{ConfigValue, PluginContext, SpoeMessage, SpoeValue};

use maxmind_plugin::config::PluginConfig;

const FIXTURES: &str = "tests/fixtures/MaxMind-DB/test-data";

fn fixture_path(name: &str) -> String {
    Path::new(FIXTURES)
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn make_context(params: &[(&str, ConfigValue)]) -> PluginContext {
    let mut config = RHashMap::new();
    for (k, v) in params {
        config.insert(RString::from(*k), v.clone());
    }
    PluginContext {
        name: RString::from("maxmind"),
        config,
    }
}

fn make_databases_config(entries: &[(&str, &str)]) -> ConfigValue {
    let arr: RVec<ConfigValue> = entries
        .iter()
        .map(|(db_type, path)| {
            let mut table = RHashMap::new();
            table.insert(
                RString::from("type"),
                ConfigValue::String(RString::from(*db_type)),
            );
            table.insert(
                RString::from("path"),
                ConfigValue::String(RString::from(*path)),
            );
            ConfigValue::Table(table)
        })
        .collect();
    ConfigValue::Array(arr)
}

// --- T014: IP extraction from SpoeValue ---

#[test]
fn extract_ip_from_ipv4_value() {
    let ip_bytes: [u8; 4] = Ipv4Addr::new(89, 160, 20, 128).octets();
    let value = SpoeValue::Ipv4(ip_bytes);
    let ip = maxmind_plugin::ip::spoe_value_to_ip(&value).unwrap();
    assert_eq!(ip.to_string(), "89.160.20.128");
}

#[test]
fn extract_ip_from_ipv6_value() {
    let ip: std::net::Ipv6Addr = "2001:218::".parse().unwrap();
    let value = SpoeValue::Ipv6(ip.octets());
    let result = maxmind_plugin::ip::spoe_value_to_ip(&value).unwrap();
    assert_eq!(result.to_string(), "2001:218::");
}

#[test]
fn extract_ip_from_string_value() {
    let value = SpoeValue::String(RString::from("89.160.20.128"));
    let ip = maxmind_plugin::ip::spoe_value_to_ip(&value).unwrap();
    assert_eq!(ip.to_string(), "89.160.20.128");
}

#[test]
fn extract_ip_from_invalid_string_returns_error() {
    let value = SpoeValue::String(RString::from("not-an-ip"));
    let result = maxmind_plugin::ip::spoe_value_to_ip(&value);
    assert!(result.is_err());
}

#[test]
fn extract_ip_from_unsupported_type_returns_error() {
    let value = SpoeValue::Int32(42);
    let result = maxmind_plugin::ip::spoe_value_to_ip(&value);
    assert!(result.is_err());
}

// --- T021: Config parsing with single database ---

#[test]
fn config_single_database_parses_correctly() {
    let city_path = fixture_path("GeoIP2-City-Test.mmdb");
    let ctx = make_context(&[("databases", make_databases_config(&[("city", &city_path)]))]);
    let config = PluginConfig::from_context(&ctx).unwrap();
    assert_eq!(config.databases.len(), 1);
    assert_eq!(config.locale, "en");
    assert!(config.fields.is_none());
}

// --- T022: Config parsing with multiple databases ---

#[test]
fn config_multiple_databases_parses_correctly() {
    let city_path = fixture_path("GeoIP2-City-Test.mmdb");
    let asn_path = fixture_path("GeoLite2-ASN-Test.mmdb");
    let ctx = make_context(&[(
        "databases",
        make_databases_config(&[("city", &city_path), ("asn", &asn_path)]),
    )]);
    let config = PluginConfig::from_context(&ctx).unwrap();
    assert_eq!(config.databases.len(), 2);
}

// --- T023: Missing database path returns error ---

#[test]
fn config_missing_databases_key_returns_error() {
    let ctx = make_context(&[]);
    let result = PluginConfig::from_context(&ctx);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("databases"));
}

#[test]
fn config_unknown_database_type_returns_error() {
    let ctx = make_context(&[(
        "databases",
        make_databases_config(&[("unknown_type", "/tmp/fake.mmdb")]),
    )]);
    let result = PluginConfig::from_context(&ctx);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("unknown database type"));
}

// --- T024: Config schema is valid JSON ---

#[test]
fn config_schema_is_valid_json() {
    let schema = PluginConfig::config_schema();
    let parsed: serde_json::Value =
        serde_json::from_str(schema).expect("config_schema() should return valid JSON");
    assert_eq!(parsed["type"], "object");
    assert!(parsed["properties"]["databases"].is_object());
}

// --- T015: Missing IP argument ---

#[test]
fn missing_ip_argument_in_message() {
    let message = SpoeMessage {
        name: RString::from("geoip-lookup"),
        args: RHashMap::new(),
        stream_id: 1,
        frame_id: 1,
    };
    let result = maxmind_plugin::ip::extract_ip(&message);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("missing required 'ip' argument")
    );
}

// --- Config with locale and fields ---

#[test]
fn config_with_locale_and_fields() {
    let city_path = fixture_path("GeoIP2-City-Test.mmdb");
    let ctx = make_context(&[
        ("databases", make_databases_config(&[("city", &city_path)])),
        ("locale", ConfigValue::String(RString::from("de"))),
        (
            "fields",
            ConfigValue::Array(
                vec![
                    ConfigValue::String(RString::from("country_iso_code")),
                    ConfigValue::String(RString::from("city_name")),
                ]
                .into(),
            ),
        ),
    ]);
    let config = PluginConfig::from_context(&ctx).unwrap();
    assert_eq!(config.locale, "de");
    assert_eq!(config.fields.as_ref().unwrap().len(), 2);
}
